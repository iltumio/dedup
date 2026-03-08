use std::path::PathBuf;
use std::sync::Mutex;

use dedup_core::{DirEntry, FileMetadata, ScanProgress, ScanStats, Store};
use tauri::{AppHandle, Emitter, State};

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
pub fn open_file(state: State<'_, AppState>, path: String) -> Result<(), String> {
    state.ensure_store()?;
    let store_guard = state.store.lock().map_err(|e| e.to_string())?;
    let store = store_guard
        .as_ref()
        .ok_or_else(|| "No store loaded.".to_string())?;

    let data = store.read_file(&path).map_err(|e| e.to_string())?;

    // Extract the original filename from the virtual path
    let filename = path
        .rsplit('/')
        .next()
        .unwrap_or("file");

    // Write to a temp file preserving the original name so the OS picks the right app
    let tmp_dir = std::env::temp_dir().join("dedup-preview");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let tmp_path = tmp_dir.join(filename);
    std::fs::write(&tmp_path, &data).map_err(|e| e.to_string())?;

    open::that(&tmp_path).map_err(|e| e.to_string())?;

    Ok(())
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

/// Scan a source directory into the store, optionally under a target virtual path.
///
/// - `source`: absolute path to the directory to scan
/// - `store_path`: path to the .store directory
/// - `target_path`: virtual path to place content under (e.g. "/photos/vacation", or "/" for root)
///
/// Emits `scan-progress` events via the app handle for real-time UI updates.
/// Existing store content is preserved (incremental).
/// Runs on a background thread so the UI stays responsive.
#[tauri::command]
pub async fn scan_directory(
    app: AppHandle,
    state: State<'_, AppState>,
    source: String,
    store_path: String,
    target_path: String,
) -> Result<ScanStats, String> {
    let store_path_buf = PathBuf::from(&store_path);
    let source_path = PathBuf::from(&source);

    // Take the store out of state (or open a fresh one).
    // redb locks the DB file, so we can't have two Store handles simultaneously.
    let store = {
        let current_store_path = state.store_path.lock().map_err(|e| e.to_string())?;
        let mut store_guard = state.store.lock().map_err(|e| e.to_string())?;

        if store_guard.is_some() && *current_store_path == store_path_buf {
            // Take ownership — state becomes None temporarily
            store_guard.take().unwrap()
        } else {
            // Drop existing store (if any) before opening new path
            store_guard.take();
            Store::open(&store_path_buf).map_err(|e| e.to_string())?
        }
    };

    // Update the stored path
    {
        let mut sp = state.store_path.lock().map_err(|e| e.to_string())?;
        *sp = store_path_buf.clone();
    }

    // Spawn the heavy scanning work on a blocking thread
    let result = tokio::task::spawn_blocking(move || {
        let stats = store
            .scan_into(
                &source_path,
                &target_path,
                move |progress: &ScanProgress| {
                    let _ = app.emit("scan-progress", progress.clone());
                },
            )
            .map_err(|e| e.to_string());
        (store, stats)
    })
    .await
    .map_err(|e| format!("Scan task failed: {e}"))?;

    let (store, stats) = result;

    // Put the store back into state so subsequent queries work
    {
        let mut store_guard = state.store.lock().map_err(|e| e.to_string())?;
        *store_guard = Some(store);
    }

    stats
}
