//! Windows portable updates never invoke an installer. A copy of the current
//! executable waits for the app to exit, replaces that exact file, and relaunches
//! it. Keeping the helper separate also lets us restore the old exe on failure.

use std::{
    fs, io,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

#[cfg(windows)]
use std::{ffi::OsString, process::Command};

#[cfg(windows)]
const APPLY: &str = "--iec104-apply-portable-update";
#[cfg(windows)]
const CLEANUP: &str = "--iec104-cleanup-portable-update";
const STAGE_PREFIX: &str = ".iec104-update-";

pub fn target_for_bundle(installed: bool, arch: &str) -> Option<String> {
    // The bundler patches NSIS/MSI copies; the original cargo executable keeps
    // the unknown bundle marker. This also survives users renaming the exe.
    (!installed).then(|| format!("windows-{arch}-portable"))
}

pub fn validate_download(target: &str, path: &str) -> Result<(), String> {
    if !target.ends_with("-portable") || !path.ends_with("-portable.exe") {
        return Err("portable update did not contain a portable executable".into());
    }
    Ok(())
}

fn validate_stage(stage: &Path, target: &Path) -> io::Result<()> {
    let valid_name = stage
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.starts_with(STAGE_PREFIX));
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::other("missing executable directory"))?;
    if !valid_name || stage.canonicalize()?.parent() != Some(parent.canonicalize()?.as_path()) {
        return Err(io::Error::other(
            "invalid portable update staging directory",
        ));
    }
    Ok(())
}

fn retry_rename(from: &Path, to: &Path) -> io::Result<()> {
    for attempt in 0..50 {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) if attempt == 49 => return Err(error),
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
    unreachable!()
}

fn replace_and_launch(
    stage: &Path,
    target: &Path,
    launch: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    validate_stage(stage, target)?;
    let candidate = stage.join("new.exe");
    let backup = stage.join("old.exe");
    if backup.exists() {
        return Err(io::Error::other("portable update backup already exists"));
    }
    retry_rename(target, &backup)?;
    let install = fs::rename(&candidate, target).and_then(|()| launch(target));
    if let Err(error) = install {
        // Move a failed new exe out of the way before restoring the original.
        if target.exists() {
            retry_rename(target, &candidate)?;
        }
        retry_rename(&backup, target)?;
        return Err(error);
    }
    Ok(())
}

#[cfg(windows)]
fn launch(target: &Path, stage: &Path, args: &[OsString]) -> io::Result<()> {
    Command::new(target)
        .arg(CLEANUP)
        .arg(stage)
        .args(args)
        .spawn()?;
    Ok(())
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
    fn WaitForSingleObject(handle: *mut std::ffi::c_void, timeout_ms: u32) -> u32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
}

fn unique_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{}-{nanos}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(windows)]
struct ParentProcess(*mut std::ffi::c_void);

#[cfg(windows)]
impl ParentProcess {
    fn open(pid: u32) -> io::Result<Self> {
        // Only synchronization rights are needed; no elevation or process writes.
        let handle = unsafe { OpenProcess(0x00100000, 0, pid) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }

    fn wait(&self) -> io::Result<()> {
        match unsafe { WaitForSingleObject(self.0, 60_000) } {
            0 => Ok(()),
            0x102 => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "app did not exit for portable update",
            )),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

#[cfg(windows)]
impl Drop for ParentProcess {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// Called before Tauri creates any windows or background tasks.
#[cfg(windows)]
pub fn handle_startup() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|s| s == APPLY) {
        let result = apply_from_args(&args);
        if let Err(error) = &result {
            eprintln!("portable update failed: {error}");
        }
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    if args.first().is_some_and(|s| s == CLEANUP) {
        if let (Some(stage), Ok(target)) = (args.get(1), std::env::current_exe()) {
            let stage = PathBuf::from(stage);
            if validate_stage(&stage, &target).is_ok() {
                thread::spawn(move || {
                    // The helper can still be returning from CreateProcess.
                    for _ in 0..50 {
                        if fs::remove_dir_all(&stage).is_ok() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                });
            }
        }
    }
}

#[cfg(windows)]
fn apply_from_args(args: &[OsString]) -> io::Result<()> {
    let pid = args
        .get(1)
        .and_then(|s| s.to_str())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| io::Error::other("missing parent process id"))?;
    let target = PathBuf::from(
        args.get(2)
            .ok_or_else(|| io::Error::other("missing target executable"))?,
    );
    let helper = std::env::current_exe()?;
    let stage = helper
        .parent()
        .ok_or_else(|| io::Error::other("missing helper directory"))?;
    validate_stage(stage, &target)?;
    // Open the parent before acknowledging readiness, avoiding PID reuse races.
    let parent = ParentProcess::open(pid)?;
    fs::write(stage.join("ready"), [])?;
    parent.wait()?;
    if let Err(error) = replace_and_launch(stage, &target, |exe| launch(exe, stage, &args[3..])) {
        let _ = fs::write(stage.join("error.txt"), error.to_string());
        // The replacement routine restored the old exe if the new one could not
        // be launched. Leave recovery files in place for inspection on failure.
        let _ = Command::new(&target).args(&args[3..]).spawn();
        return Err(error);
    }
    Ok(())
}

/// Stage bytes already verified by Tauri's updater, then acknowledge that the
/// helper is alive and waiting before allowing the app to exit.
#[cfg(windows)]
pub fn start(bytes: &[u8]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let target = std::env::current_exe().map_err(|e| e.to_string())?;
    let stage = target
        .parent()
        .ok_or("missing executable directory")?
        .join(format!("{STAGE_PREFIX}{}", unique_id()));
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let result = (|| -> io::Result<()> {
        fs::write(stage.join("new.exe"), bytes)?;
        let helper = stage.join("helper.exe");
        fs::copy(&target, &helper)?;
        let mut args: Vec<_> = std::env::args_os().skip(1).collect();
        if args.first().is_some_and(|s| s == CLEANUP) {
            args.drain(..2.min(args.len()));
        }
        let mut child = Command::new(helper)
            .arg(APPLY)
            .arg(std::process::id().to_string())
            .arg(&target)
            .args(args)
            .creation_flags(0x08000000)
            .spawn()?;
        for _ in 0..100 {
            if child.try_wait()?.is_some() {
                return Err(io::Error::other(
                    "portable update helper exited before it was ready",
                ));
            }
            if stage.join("ready").exists() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "portable update helper did not become ready",
        ))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
        stage: PathBuf,
        target: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("portable updater 中文 space {}", unique_id()));
            let stage = root.join(format!("{STAGE_PREFIX}test"));
            fs::create_dir_all(&stage).unwrap();
            let target = root.join("renamed app.exe");
            fs::write(&target, b"original").unwrap();
            fs::write(stage.join("new.exe"), b"new version").unwrap();
            Self {
                root,
                stage,
                target,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn portable_targets_never_fall_back_to_installers() {
        assert_eq!(
            target_for_bundle(false, "x86_64").as_deref(),
            Some("windows-x86_64-portable")
        );
        assert_eq!(
            target_for_bundle(false, "aarch64").as_deref(),
            Some("windows-aarch64-portable")
        );
        assert_eq!(target_for_bundle(true, "x86_64"), None);
        assert_eq!(target_for_bundle(true, "aarch64"), None);
        assert!(validate_download("windows-x86_64", "/app_x64-setup.exe").is_err());
        assert!(validate_download("windows-x86_64-portable", "/app_x64-setup.exe").is_err());
        assert!(validate_download("windows-x86_64-portable", "/app_x64-portable.exe").is_ok());
    }

    #[test]
    fn replaces_the_same_filename_and_retains_a_backup() {
        let f = Fixture::new();
        replace_and_launch(&f.stage, &f.target, |path| {
            assert_eq!(path, f.target);
            assert_eq!(fs::read(path)?, b"new version");
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read(&f.target).unwrap(), b"new version");
        assert_eq!(fs::read(f.stage.join("old.exe")).unwrap(), b"original");
    }

    #[test]
    fn restores_the_original_when_launch_fails() {
        let f = Fixture::new();
        assert!(
            replace_and_launch(&f.stage, &f.target, |_| Err(io::Error::other(
                "launch failed"
            )))
            .is_err()
        );
        assert_eq!(fs::read(&f.target).unwrap(), b"original");
        assert_eq!(fs::read(f.stage.join("new.exe")).unwrap(), b"new version");
    }

    #[test]
    fn restores_the_original_when_candidate_is_missing() {
        let f = Fixture::new();
        fs::remove_file(f.stage.join("new.exe")).unwrap();
        assert!(replace_and_launch(&f.stage, &f.target, |_| panic!("must not launch")).is_err());
        assert_eq!(fs::read(&f.target).unwrap(), b"original");
    }

    #[test]
    fn refuses_to_overwrite_a_previous_backup() {
        let f = Fixture::new();
        fs::write(f.stage.join("old.exe"), b"saved recovery copy").unwrap();
        assert!(replace_and_launch(&f.stage, &f.target, |_| panic!("must not launch")).is_err());
        assert_eq!(fs::read(&f.target).unwrap(), b"original");
        assert_eq!(
            fs::read(f.stage.join("old.exe")).unwrap(),
            b"saved recovery copy"
        );
    }

    #[test]
    fn refuses_a_target_outside_the_staging_parent() {
        let f = Fixture::new();
        let outside = f.root.join("other");
        fs::create_dir(&outside).unwrap();
        let target = outside.join("app.exe");
        fs::write(&target, b"unrelated").unwrap();
        assert!(replace_and_launch(&f.stage, &target, |_| panic!("must not launch")).is_err());
        assert_eq!(fs::read(target).unwrap(), b"unrelated");
    }

    #[cfg(windows)]
    #[test]
    fn native_helper_replaces_and_relaunches_a_renamed_exe() {
        run_native_fixture(false);
    }

    #[cfg(windows)]
    #[test]
    fn native_helper_restores_and_relaunches_after_invalid_exe() {
        run_native_fixture(true);
    }

    #[cfg(windows)]
    fn run_native_fixture(rollback: bool) {
        let f = Fixture::new();
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../shared/portable_update_fixture.rs");
        let candidate = f.root.join("candidate.exe");
        for (output, updated) in [(&f.target, false), (&candidate, true)] {
            if updated && rollback {
                fs::write(output, b"not a Windows executable").unwrap();
                continue;
            }
            let mut compiler = Command::new("rustc");
            compiler
                .arg("--edition=2021")
                .arg(&source)
                .arg("-o")
                .arg(output);
            if updated {
                compiler.arg("--cfg").arg("updated_fixture");
            }
            let result = compiler.output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let original = fs::read(&f.target).unwrap();
        let marker = f.root.join("restarted.txt");
        let status = Command::new(&f.target)
            .args(["--user-argument", "中文 path with spaces"])
            .env("IEC104_FIXTURE_CANDIDATE", &candidate)
            .env("IEC104_FIXTURE_MARKER", &marker)
            .current_dir(&f.root)
            .status()
            .unwrap();
        assert!(status.success());
        for _ in 0..200 {
            if marker.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let result = fs::read_to_string(&marker).expect("updated executable was not launched");
        let pid = result.lines().next().unwrap().parse().unwrap();
        if let Ok(process) = ParentProcess::open(pid) {
            process.wait().unwrap();
        }
        assert!(result.contains("中文 path with spaces"), "{result}");
        assert!(result.contains("renamed app.exe"), "{result}");
        if rollback {
            assert!(result.contains("rollback=true"), "{result}");
            assert_eq!(fs::read(&f.target).unwrap(), original);
            return;
        }
        assert_eq!(fs::read(&f.target).unwrap(), fs::read(&candidate).unwrap());
        for _ in 0..100 {
            let remaining = fs::read_dir(&f.root)
                .unwrap()
                .filter_map(Result::ok)
                .any(|e| {
                    e.file_name().to_string_lossy().starts_with(STAGE_PREFIX) && e.path() != f.stage
                });
            if !remaining {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("helper did not clean up its staging files");
    }
}
