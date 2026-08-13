use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_store::StoreExt;
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

const STORE_FILE: &str = "update_state.json";
const KEY_LAST_CHECK: &str = "last_check_at";
const KEY_SKIPPED_VERSION: &str = "skipped_version";
const KEY_INSTALL_ON_NEXT_LAUNCH: &str = "install_on_next_launch";
const KEY_INSTALL_ID: &str = "install_id";
const THROTTLE_HOURS: i64 = 6;

#[derive(Serialize, Clone)]
pub struct UpdateMeta {
    pub version: String,
    pub notes: String,
    pub pub_date: Option<String>,
}

struct PreparedUpdate {
    meta: UpdateMeta,
    update: Update,
    bytes: Vec<u8>,
}

/// Serializes updater operations and keeps the signature-verified package in
/// memory until the user chooses what to do with it.
#[derive(Default)]
pub struct UpdateState {
    prepared: Mutex<Option<PreparedUpdate>>,
}

fn read_str(app: &AppHandle, key: &str) -> Option<String> {
    let store = app.store(STORE_FILE).ok()?;
    store.get(key).and_then(|v| v.as_str().map(String::from))
}

fn read_bool(app: &AppHandle, key: &str) -> bool {
    let Ok(store) = app.store(STORE_FILE) else {
        return false;
    };
    store.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn write_str(app: &AppHandle, key: &str, value: &str) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.set(key, serde_json::Value::String(value.to_string()));
        let _ = store.save();
    }
}

fn write_bool(app: &AppHandle, key: &str, value: bool) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.set(key, serde_json::Value::Bool(value));
        let _ = store.save();
    }
}

fn remove_value(app: &AppHandle, key: &str) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.delete(key);
        let _ = store.save();
    }
}

fn parse_ts(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

// 匿名安装 ID:首次启动生成 UUID 并持久化,随更新检查经 X-Install-Id 头
// 上报到 cn0 加速源做匿名活跃统计,不含任何个人信息。
fn install_id(app: &AppHandle) -> String {
    if let Some(id) = read_str(app, KEY_INSTALL_ID) {
        return id;
    }
    let id = uuid::Uuid::new_v4().to_string();
    write_str(app, KEY_INSTALL_ID, &id);
    id
}

fn build_updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    app.updater_builder()
        .header("X-Install-Id", install_id(app))
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())
}

fn update_meta(update: &Update) -> UpdateMeta {
    UpdateMeta {
        version: update.version.clone(),
        notes: update.body.clone().unwrap_or_default(),
        pub_date: update.date.map(|d| d.to_string()),
    }
}

async fn download_update(update: &Update) -> Result<Vec<u8>, String> {
    update
        .download(
            |_, _| {},
            || {
                log::info!("update download finished; verifying release signature");
            },
        )
        .await
        .map_err(|e| e.to_string())
}

// `force = true` (toolbar button) bypasses the 6h throttle and a skipped
// version. The command only returns metadata after the package is fully
// downloaded and signature-verified, so the frontend dialog always means
// "ready to install".
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: State<'_, UpdateState>,
    force: Option<bool>,
) -> Result<Option<UpdateMeta>, String> {
    let force = force.unwrap_or(false);

    // A choice made in the previous run is handled by the startup task. Do not
    // race it with the regular automatic check.
    if !force && read_bool(&app, KEY_INSTALL_ON_NEXT_LAUNCH) {
        return Ok(None);
    }

    let mut prepared = state.prepared.lock().await;
    if let Some(update) = prepared.as_ref() {
        return Ok(Some(update.meta.clone()));
    }

    let now = Utc::now();
    if !force {
        let last = parse_ts(read_str(&app, KEY_LAST_CHECK));
        if !should_check(last, now, Duration::hours(THROTTLE_HOURS)) {
            return Ok(None);
        }
    }
    write_str(&app, KEY_LAST_CHECK, &now.to_rfc3339());

    let updater = build_updater(&app)?;
    // Surface fetch / parse / download failures to the caller so a manual
    // check can distinguish them from "already latest". Startup checks log
    // and suppress those failures in the frontend.
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };

    if !force
        && is_skipped(
            read_str(&app, KEY_SKIPPED_VERSION).as_deref(),
            &update.version,
        )
    {
        return Ok(None);
    }

    let meta = update_meta(&update);
    let bytes = download_update(&update).await?;
    *prepared = Some(PreparedUpdate {
        meta: meta.clone(),
        update,
        bytes,
    });
    Ok(Some(meta))
}

/// Installs the already downloaded package. No network request is made here.
#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, UpdateState>) -> Result<(), String> {
    let prepared = state.prepared.lock().await;
    let update = prepared
        .as_ref()
        .ok_or_else(|| "update package is not ready".to_string())?;

    update
        .update
        .install(&update.bytes)
        .map_err(|e| e.to_string())?;
    remove_value(&app, KEY_SKIPPED_VERSION);
    remove_value(&app, KEY_INSTALL_ON_NEXT_LAUNCH);
    drop(prepared);
    app.restart()
}

/// Ignores this exact version during future automatic checks. A manual check
/// can still surface it, and a newer version is never hidden.
#[tauri::command]
pub async fn skip_update(
    app: AppHandle,
    state: State<'_, UpdateState>,
    version: String,
) -> Result<(), String> {
    let mut prepared = state.prepared.lock().await;
    if !prepared
        .as_ref()
        .is_some_and(|update| update.meta.version == version)
    {
        return Err("update package is not ready".to_string());
    }
    *prepared = None;
    write_str(&app, KEY_SKIPPED_VERSION, &version);
    remove_value(&app, KEY_INSTALL_ON_NEXT_LAUNCH);
    Ok(())
}

/// Records a one-shot choice. The next process launch re-downloads the package
/// so Tauri verifies its release signature again before installing it.
#[tauri::command]
pub async fn schedule_update_on_next_launch(
    app: AppHandle,
    state: State<'_, UpdateState>,
    version: String,
) -> Result<(), String> {
    let prepared = state.prepared.lock().await;
    if !prepared
        .as_ref()
        .is_some_and(|update| update.meta.version == version)
    {
        return Err("update package is not ready".to_string());
    }
    write_bool(&app, KEY_INSTALL_ON_NEXT_LAUNCH, true);
    remove_value(&app, KEY_SKIPPED_VERSION);
    Ok(())
}

/// Called from Tauri setup on every launch. It is a no-op unless the user chose
/// "install on next launch" in the previous run.
pub async fn install_pending_update(app: AppHandle) -> Result<(), String> {
    if !read_bool(&app, KEY_INSTALL_ON_NEXT_LAUNCH) {
        return Ok(());
    }

    let state = app.state::<UpdateState>();
    let mut prepared = state.prepared.lock().await;
    let updater = build_updater(&app)?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        remove_value(&app, KEY_INSTALL_ON_NEXT_LAUNCH);
        return Ok(());
    };

    let meta = update_meta(&update);
    let bytes = download_update(&update).await?;
    *prepared = Some(PreparedUpdate {
        meta,
        update,
        bytes,
    });
    let ready = prepared
        .as_ref()
        .expect("prepared update was just inserted");
    ready
        .update
        .install(&ready.bytes)
        .map_err(|e| e.to_string())?;

    remove_value(&app, KEY_SKIPPED_VERSION);
    remove_value(&app, KEY_INSTALL_ON_NEXT_LAUNCH);
    drop(prepared);
    app.restart()
}

pub fn should_check(
    last_check: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    throttle: Duration,
) -> bool {
    match last_check {
        None => true,
        Some(last) => now - last >= throttle,
    }
}

pub fn is_skipped(skipped_version: Option<&str>, remote_version: &str) -> bool {
    skipped_version == Some(remote_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_check_respects_throttle_boundary() {
        let now = Utc::now();
        let throttle = Duration::hours(THROTTLE_HOURS);
        assert!(!should_check(
            Some(now - throttle + Duration::seconds(1)),
            now,
            throttle
        ));
        assert!(should_check(Some(now - throttle), now, throttle));
    }

    #[test]
    fn skipped_version_only_hides_the_exact_release() {
        assert!(is_skipped(Some("1.2.3"), "1.2.3"));
        assert!(!is_skipped(Some("1.2.3"), "1.2.4"));
        assert!(!is_skipped(None, "1.2.3"));
    }
}
