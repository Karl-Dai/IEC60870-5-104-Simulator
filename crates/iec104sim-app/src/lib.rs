mod commands;
mod point_csv;
mod state;
pub mod update;

use state::AppState;
use tauri::Manager;

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
            // Server commands
            commands::create_server,
            commands::create_and_start_server,
            commands::start_server,
            commands::stop_server,
            commands::delete_server,
            commands::list_servers,
            commands::list_client_connections,
            commands::update_server_transport,
            commands::get_server_transport,
            commands::list_bind_address_suggestions,
            // Station commands
            commands::add_station,
            commands::update_station,
            commands::remove_station,
            commands::list_stations,
            // Data point commands
            commands::add_data_point,
            commands::update_data_point_definition,
            commands::list_control_mapping_targets,
            commands::batch_add_data_points,
            commands::remove_data_point,
            commands::batch_remove_data_points,
            commands::batch_migrate_data_point_types,
            commands::batch_update_control_options,
            commands::update_data_point,
            commands::set_data_point_quality,
            commands::batch_set_data_point_quality,
            commands::batch_update_data_points,
            commands::list_data_points,
            commands::list_data_points_since,
            commands::get_data_point,
            commands::get_data_point_values,
            // Log commands
            commands::get_communication_logs,
            commands::clear_communication_logs,
            commands::export_logs_csv,
            commands::save_logs_csv,
            // Simulation commands
            commands::random_mutate_data_points,
            commands::set_cyclic_config,
            // Remote operation configuration (远动运行参数)
            commands::set_protocol_timing,
            commands::get_protocol_timing,
            commands::set_remote_operation_config,
            commands::get_remote_operation_config,
            commands::start_point_mutation,
            commands::stop_point_mutation,
            commands::list_point_mutations,
            // Config file save/open
            commands::save_config,
            commands::load_config,
            // Per-station point CSV import/export (JSON config remains separate)
            point_csv::save_point_config_csv,
            point_csv::import_point_config_csv,
            point_csv::save_point_config_csv_template,
            // Tool commands
            commands::parse_hex,
            commands::parse_apci,
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
                if let Err(error) =
                    commands::restore_persisted_workspace(app_handle.clone()).await
                {
                    log::warn!("failed to restore persisted slave workspace: {error}");
                }
                app_handle.state::<AppState>().mark_workspace_ready();
                if let Err(error) = update::install_pending_update(app_handle).await {
                    log::warn!("automatic update on launch failed: {error}");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
