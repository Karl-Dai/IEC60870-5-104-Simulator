//! Native process fixture for the Windows CI test. This does not open Tauri UI.
#![allow(dead_code)]
mod portable_update;

fn main() {
    portable_update::handle_startup();
    let failed_update = std::fs::read_dir(std::env::current_exe().unwrap().parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| e.path().join("error.txt").is_file());
    if cfg!(updated_fixture) || failed_update {
        let marker = std::env::var_os("IEC104_FIXTURE_MARKER").unwrap();
        let result = format!(
            "{}\n{}\n{:?}\nrollback={failed_update}",
            std::process::id(),
            std::env::current_exe().unwrap().display(),
            std::env::args_os().collect::<Vec<_>>()
        );
        std::fs::write(marker, result).unwrap();
        // Give the background cleanup time to observe the helper's exit.
        std::thread::sleep(std::time::Duration::from_secs(1));
    } else {
        let candidate = std::env::var_os("IEC104_FIXTURE_CANDIDATE").unwrap();
        portable_update::start(&std::fs::read(candidate).unwrap()).unwrap();
    }
}
