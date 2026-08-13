mod commands;
pub mod reconnect;
mod state;
pub mod update;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .manage(update::UpdateState::default())
        .invoke_handler(tauri::generate_handler![
            // Connection commands
            commands::create_connection,
            commands::connect_master,
            commands::disconnect_master,
            commands::delete_connection,
            commands::list_connections,
            // IEC 104 commands
            commands::send_interrogation,
            commands::send_interrogation_deactivation,
            commands::send_clock_sync,
            commands::send_counter_read,
            commands::send_counter_read_deactivation,
            commands::send_broadcast_gi,
            commands::send_broadcast_clock_sync,
            commands::send_broadcast_counter_read,
            commands::send_broadcast_gi_deactivation,
            commands::send_broadcast_counter_read_deactivation,
            commands::send_control_command,
            // Data commands
            commands::get_received_data,
            commands::get_received_data_since,
            // Log commands
            commands::get_communication_logs,
            commands::clear_communication_logs,
            commands::export_logs_csv,
            commands::save_logs_csv,
            commands::set_logging_enabled,
            // Config file save/open
            commands::save_config,
            commands::load_config,
            // Tool commands
            commands::parse_hex,
            commands::parse_frame_full,
            // Update commands
            update::check_for_update,
            update::install_update,
            update::skip_update,
            update::schedule_update_on_next_launch,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = update::install_pending_update(app_handle).await {
                    log::warn!("automatic update on launch failed: {error}");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
