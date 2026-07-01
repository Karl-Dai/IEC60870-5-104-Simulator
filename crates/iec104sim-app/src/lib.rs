mod commands;
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
        .invoke_handler(tauri::generate_handler![
            // Server commands
            commands::create_server,
            commands::start_server,
            commands::stop_server,
            commands::delete_server,
            commands::list_servers,
            commands::update_server_transport,
            // Station commands
            commands::add_station,
            commands::remove_station,
            commands::list_stations,
            // Data point commands
            commands::add_data_point,
            commands::batch_add_data_points,
            commands::remove_data_point,
            commands::batch_remove_data_points,
            commands::update_data_point,
            commands::set_data_point_quality,
            commands::batch_set_data_point_quality,
            commands::batch_update_data_points,
            commands::list_data_points,
            commands::list_data_points_since,
            commands::get_data_point,
            // Log commands
            commands::get_communication_logs,
            commands::clear_communication_logs,
            commands::export_logs_csv,
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
            // Tool commands
            commands::parse_hex,
            commands::parse_apci,
            commands::parse_frame_full,
            // Update commands
            update::check_for_update,
            update::install_update,
            update::snooze_update,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
