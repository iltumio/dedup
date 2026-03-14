// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod workspace;

use commands::AppState;

fn main() {
    let config_path = workspace::default_config_path();

    tauri::Builder::default()
        .manage(AppState::new(config_path))
        .invoke_handler(tauri::generate_handler![
            commands::list_dir,
            commands::get_file_metadata,
            commands::read_file,
            commands::find_duplicates,
            commands::find_all_duplicates,
            commands::scan_directory,
            commands::open_file,
            commands::list_workspaces,
            commands::create_workspace,
            commands::switch_workspace,
            commands::delete_workspace,
            commands::export_workspaces,
            commands::import_workspaces,
            commands::get_extension_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running dedup app");
}
