use serde::Deserialize;
use tauri::{Manager, Window};
use tauri_plugin_dialog::DialogExt;

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TlsFileKind {
    Certificate,
    PrivateKey,
}

/// Use an independent macOS panel so certificate paths can be browsed in a
/// movable window. The plugin's JS open() always attaches a sheet on macOS.
#[tauri::command]
pub async fn choose_tls_file(
    window: Window,
    kind: TlsFileKind,
    title: String,
    filter_name: String,
) -> Result<Option<String>, String> {
    let extensions: &[&str] = match kind {
        TlsFileKind::Certificate => &["crt", "cer", "pem"],
        TlsFileKind::PrivateKey => &["key", "pem"],
    };
    let dialog = window
        .dialog()
        .file()
        .set_title(title)
        .add_filter(filter_name, extensions);
    // Windows retains its owner window; macOS must have no parent to avoid a sheet.
    #[cfg(windows)]
    let dialog = dialog.set_parent(&window);

    let (send, receive) = tokio::sync::oneshot::channel();
    dialog.pick_file(move |selected| {
        let _ = send.send(selected);
    });
    receive
        .await
        .map_err(|error| error.to_string())?
        .map(|file| {
            let path = file.into_path().map_err(|error| error.to_string())?;
            window
                .state::<tauri::scope::Scopes>()
                .allow_file(&path)
                .map_err(|error| error.to_string())?;
            Ok(path.to_string_lossy().into_owned())
        })
        .transpose()
}
