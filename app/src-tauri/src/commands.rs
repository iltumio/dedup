use std::path::PathBuf;
use std::sync::Mutex;

use dedup_core::{DirEntry, ExtensionStats, FileMetadata, ScanProgress, ScanStats, Store};
use tauri::{AppHandle, Emitter, State};

use crate::workspace::{self, Workspace, WorkspacesConfig};

/// Shared application state holding the current store and workspaces.
pub struct AppState {
    pub store: Mutex<Option<Store>>,
    pub store_path: Mutex<PathBuf>,
    pub workspaces: Mutex<WorkspacesConfig>,
    pub config_path: PathBuf,
}

impl AppState {
    pub fn new(config_path: PathBuf) -> Self {
        let config = WorkspacesConfig::load(&config_path).unwrap_or_default();
        let initial_store_path = config
            .active()
            .map(|w| PathBuf::from(&w.store_path))
            .unwrap_or_else(|| PathBuf::from(".store"));

        Self {
            store: Mutex::new(None),
            store_path: Mutex::new(initial_store_path),
            workspaces: Mutex::new(config),
            config_path,
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

    fn save_config(&self) -> Result<(), String> {
        let config = self.workspaces.lock().map_err(|e| e.to_string())?;
        config.save(&self.config_path)
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

#[tauri::command]
pub fn get_extension_stats(state: State<'_, AppState>) -> Result<Vec<ExtensionStats>, String> {
    state.ensure_store()?;
    let store_guard = state.store.lock().map_err(|e| e.to_string())?;
    let store = store_guard
        .as_ref()
        .ok_or_else(|| "No store loaded.".to_string())?;

    store.extension_stats().map_err(|e| e.to_string())
}

/// Scan a source directory into the active workspace's store.
///
/// - `source`: absolute path to the directory to scan
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
    target_path: String,
) -> Result<ScanStats, String> {
    let store_path_buf = state.store_path.lock().map_err(|e| e.to_string())?.clone();
    let source_path = PathBuf::from(&source);

    // Take the store out of state (or open a fresh one).
    // redb locks the DB file, so we can't have two Store handles simultaneously.
    let store = {
        let mut store_guard = state.store.lock().map_err(|e| e.to_string())?;
        match store_guard.take() {
            Some(s) => s,
            None => Store::open(&store_path_buf).map_err(|e| e.to_string())?,
        }
    };

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
    let stats = stats?;

    // Put the store back into state so subsequent queries work
    {
        let mut store_guard = state.store.lock().map_err(|e| e.to_string())?;
        *store_guard = Some(store);
    }

    // Accumulate stats on the active workspace
    {
        let mut config = state.workspaces.lock().map_err(|e| e.to_string())?;
        if let Some(active_id) = config.active_workspace_id.clone() {
            if let Some(ws) = config.find_mut(&active_id) {
                ws.stats.total_files += stats.total_files;
                ws.stats.total_dirs += stats.total_dirs;
                ws.stats.unique_blobs += stats.unique_blobs;
                ws.stats.duplicate_files += stats.duplicate_files;
                ws.stats.total_original_bytes += stats.total_original_bytes;
                ws.stats.total_stored_bytes += stats.total_stored_bytes;
                ws.stats.scans_count += 1;
                ws.stats.last_scan_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
            }
        }
    }
    state.save_config()?;

    Ok(stats)
}

// ── Workspace commands ──────────────────────────────────────────────

#[tauri::command]
pub fn list_workspaces(state: State<'_, AppState>) -> Result<WorkspacesConfig, String> {
    let config = state.workspaces.lock().map_err(|e| e.to_string())?;
    Ok(config.clone())
}

#[tauri::command]
pub fn create_workspace(
    state: State<'_, AppState>,
    label: String,
    tags: Vec<String>,
    store_path: String,
) -> Result<Workspace, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let ws = Workspace {
        id: workspace::generate_id(),
        label,
        tags,
        store_path,
        created_at: now,
        stats: Default::default(),
    };

    {
        let mut config = state.workspaces.lock().map_err(|e| e.to_string())?;
        config.workspaces.push(ws.clone());
        // Auto-activate if it's the only workspace
        if config.workspaces.len() == 1 {
            config.active_workspace_id = Some(ws.id.clone());
        }
    }

    state.save_config()?;
    Ok(ws)
}

#[tauri::command]
pub fn switch_workspace(state: State<'_, AppState>, workspace_id: String) -> Result<Workspace, String> {
    let ws = {
        let mut config = state.workspaces.lock().map_err(|e| e.to_string())?;
        let ws = config
            .find(&workspace_id)
            .cloned()
            .ok_or_else(|| format!("Workspace not found: {workspace_id}"))?;
        config.active_workspace_id = Some(workspace_id);
        ws
    };

    state.save_config()?;

    // Close current store and switch to the new workspace's store path
    {
        let mut store_guard = state.store.lock().map_err(|e| e.to_string())?;
        *store_guard = None;
    }
    {
        let mut sp = state.store_path.lock().map_err(|e| e.to_string())?;
        *sp = PathBuf::from(&ws.store_path);
    }

    // Try to open the store (it may not exist yet if nothing has been scanned)
    state.ensure_store().ok();

    Ok(ws)
}

#[tauri::command]
pub fn delete_workspace(state: State<'_, AppState>, workspace_id: String) -> Result<(), String> {
    {
        let mut config = state.workspaces.lock().map_err(|e| e.to_string())?;
        config.workspaces.retain(|w| w.id != workspace_id);
        if config.active_workspace_id.as_deref() == Some(&workspace_id) {
            config.active_workspace_id = config.workspaces.first().map(|w| w.id.clone());
        }
    }

    state.save_config()?;
    Ok(())
}

#[tauri::command]
pub fn export_workspaces(state: State<'_, AppState>) -> Result<String, String> {
    let config = state.workspaces.lock().map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&*config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_workspaces(state: State<'_, AppState>, json: String) -> Result<WorkspacesConfig, String> {
    let imported: WorkspacesConfig = serde_json::from_str(&json).map_err(|e| e.to_string())?;

    {
        let mut config = state.workspaces.lock().map_err(|e| e.to_string())?;
        // Merge: add workspaces that don't already exist (by ID)
        for ws in imported.workspaces {
            if !config.workspaces.iter().any(|existing| existing.id == ws.id) {
                config.workspaces.push(ws);
            }
        }
    }

    state.save_config()?;
    let config = state.workspaces.lock().map_err(|e| e.to_string())?;
    Ok(config.clone())
}
