// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use std::path::PathBuf;

use commands::AppState;

fn main() {
    let default_store = PathBuf::from(".store");

    tauri::Builder::default()
        .manage(AppState::new(default_store))
        .invoke_handler(tauri::generate_handler![
            commands::list_dir,
            commands::get_file_metadata,
            commands::read_file,
            commands::find_duplicates,
            commands::find_all_duplicates,
            commands::scan_directory,
            commands::open_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running dedup app");
}
