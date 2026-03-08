use std::path::PathBuf;
use std::sync::Mutex;

use dedup_core::{DirEntry, FileMetadata, ScanStats, Store};
use tauri::State;

/// Shared application state holding the current store.
pub struct AppState {
    pub store: Mutex<Option<Store>>,
    pub store_path: Mutex<PathBuf>,
}

impl AppState {
    pub fn new(default_store: PathBuf) -> Self {
        Self {
            store: Mutex::new(None),
            store_path: Mutex::new(default_store),
        }
    }

    fn ensure_store(&self) -> Result<(), String> {
        let mut store = self.store.lock().map_err(|e| e.to_string())?;
        if store.is_none() {
            let path = self.store_path.lock().map_err(|e| e.to_string())?;
            if path.exists() {
                *store = Some(Store::open(&path).map_err(|e| e.to_string())?);
            }
        }
        Ok(())
    }
}

#[tauri::command]
pub fn list_dir(state: State<'_, AppState>, path: String) -> Result<Vec<DirEntry>, String> {
    state.ensure_store()?;
    let store_guard = state.store.lock().map_err(|e| e.to_string())?;
    let store = store_guard
        .as_ref()
        .ok_or_else(|| "No store loaded. Scan a directory first.".to_string())?;

    store.list_dir(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_file_metadata(
    state: State<'_, AppState>,
    path: String,
) -> Result<Option<FileMetadata>, String> {
    state.ensure_store()?;
    let store_guard = state.store.lock().map_err(|e| e.to_string())?;
    let store = store_guard
        .as_ref()
        .ok_or_else(|| "No store loaded.".to_string())?;

    store.get_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_file(state: State<'_, AppState>, path: String) -> Result<Vec<u8>, String> {
    state.ensure_store()?;
    let store_guard = state.store.lock().map_err(|e| e.to_string())?;
    let store = store_guard
        .as_ref()
        .ok_or_else(|| "No store loaded.".to_string())?;

    store.read_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn find_duplicates(state: State<'_, AppState>, path: String) -> Result<Vec<String>, String> {
    state.ensure_store()?;
    let store_guard = state.store.lock().map_err(|e| e.to_string())?;
    let store = store_guard
        .as_ref()
        .ok_or_else(|| "No store loaded.".to_string())?;

    store.find_duplicates(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn find_all_duplicates(
    state: State<'_, AppState>,
) -> Result<Vec<(String, Vec<String>)>, String> {
    state.ensure_store()?;
    let store_guard = state.store.lock().map_err(|e| e.to_string())?;
    let store = store_guard
        .as_ref()
        .ok_or_else(|| "No store loaded.".to_string())?;

    store.find_all_duplicates().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scan_directory(
    state: State<'_, AppState>,
    source: String,
    store_path: String,
) -> Result<ScanStats, String> {
    let store_path = PathBuf::from(&store_path);

    // Open or create the store at the specified path
    let new_store = Store::open(&store_path).map_err(|e| e.to_string())?;

    let source_path = PathBuf::from(&source);
    let stats = new_store.scan(&source_path).map_err(|e| e.to_string())?;

    // Update shared state
    {
        let mut sp = state.store_path.lock().map_err(|e| e.to_string())?;
        *sp = store_path;
    }
    {
        let mut store = state.store.lock().map_err(|e| e.to_string())?;
        *store = Some(new_store);
    }

    Ok(stats)
}
