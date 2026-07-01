use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use dedup_core::{
    DirEntry, ExtensionStats, FileMetadata, ScanOptions, ScanProgress, ScanRule, ScanStats, Store,
};
use regex::Regex;
use tauri::{AppHandle, Emitter, State};

use crate::workspace::{self, CustomScanRule, Workspace, WorkspacesConfig};

/// Shared application state holding the current store and workspaces.
pub struct AppState {
    pub store: Mutex<Option<Store>>,
    pub store_path: Mutex<PathBuf>,
    pub workspaces: Mutex<WorkspacesConfig>,
    pub scan_cancelled: Arc<AtomicBool>,
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
            scan_cancelled: Arc::new(AtomicBool::new(false)),
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

    fn update_config_transactionally<F>(&self, update: F) -> Result<WorkspacesConfig, String>
    where
        F: FnOnce(&mut WorkspacesConfig) -> Result<(), String>,
    {
        let mut config = self.workspaces.lock().map_err(|e| e.to_string())?;
        let previous = config.clone();

        if let Err(e) = update(&mut config) {
            *config = previous;
            return Err(e);
        }

        if let Err(e) = config.save(&self.config_path) {
            *config = previous;
            return Err(e);
        }

        Ok(config.clone())
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
    bundle_git_dirs: Option<bool>,
    rules: Option<Vec<ScanRule>>,
) -> Result<ScanStats, String> {
    let bundle_git_dirs = bundle_git_dirs.unwrap_or(false);
    let mut rules = rules.unwrap_or_default();
    if bundle_git_dirs {
        rules.insert(0, ScanRule::builtin(dedup_core::BuiltinScanPreset::Git));
    }
    state.scan_cancelled.store(false, Ordering::Relaxed);

    let store_path_buf = state.store_path.lock().map_err(|e| e.to_string())?.clone();
    let source_path = PathBuf::from(&source);
    let cancel_flag = Arc::clone(&state.scan_cancelled);

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
            .scan_into_with_options_and_cancellation(
                &source_path,
                &target_path,
                ScanOptions {
                    bundle_git_dirs: false,
                    rules,
                },
                move |progress: &ScanProgress| {
                    let _ = app.emit("scan-progress", progress.clone());
                },
                || cancel_flag.load(Ordering::Relaxed),
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
    state.scan_cancelled.store(false, Ordering::Relaxed);

    let stats = stats?;

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

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) -> Result<(), String> {
    state.scan_cancelled.store(true, Ordering::Relaxed);
    Ok(())
}

// ── Workspace commands ──────────────────────────────────────────────

fn validate_custom_scan_rule(rule: &CustomScanRule) -> Result<(), String> {
    if rule.label.trim().is_empty() {
        return Err("Rule label cannot be empty.".to_string());
    }
    if rule.pattern.trim().is_empty() {
        return Err("Rule regex cannot be empty.".to_string());
    }
    Regex::new(&rule.pattern)
        .map(|_| ())
        .map_err(|e| format!("Invalid regex: {e}"))
}

fn merge_custom_scan_rules(config: &mut WorkspacesConfig, rules: Vec<CustomScanRule>) {
    for rule in rules {
        if let Some(existing) = config
            .custom_scan_rules
            .iter_mut()
            .find(|existing| existing.id == rule.id)
        {
            *existing = rule;
        } else {
            config.custom_scan_rules.push(rule);
        }
    }
}

#[tauri::command]
pub fn list_workspaces(state: State<'_, AppState>) -> Result<WorkspacesConfig, String> {
    let config = state.workspaces.lock().map_err(|e| e.to_string())?;
    Ok(config.clone())
}

#[tauri::command]
pub fn list_custom_scan_rules(state: State<'_, AppState>) -> Result<Vec<CustomScanRule>, String> {
    let config = state.workspaces.lock().map_err(|e| e.to_string())?;
    Ok(config.custom_scan_rules.clone())
}

#[tauri::command]
pub fn save_custom_scan_rules(
    state: State<'_, AppState>,
    rules: Vec<CustomScanRule>,
) -> Result<Vec<CustomScanRule>, String> {
    for rule in &rules {
        validate_custom_scan_rule(rule)?;
    }

    let config = state.update_config_transactionally(|config| {
        config.custom_scan_rules = rules;
        Ok(())
    })?;
    Ok(config.custom_scan_rules)
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
    for rule in &imported.custom_scan_rules {
        validate_custom_scan_rule(rule)?;
    }

    state.update_config_transactionally(|config| {
        // Merge: add workspaces that don't already exist (by ID)
        for ws in imported.workspaces {
            if !config.workspaces.iter().any(|existing| existing.id == ws.id) {
                config.workspaces.push(ws);
            }
        }

        merge_custom_scan_rules(config, imported.custom_scan_rules);
        Ok(())
    })
}

#[tauri::command]
pub fn import_workspace(
 state: State<'_, AppState>,
 store_path: String,
 label: String,
) -> Result<Workspace, String> {
 let path = PathBuf::from(&store_path);

 // If the user selected a metadata.redb file, use its parent directory
 let store_dir = if path.is_file()
 && path
 .file_name()
 .map(|n| n == "metadata.redb")
 .unwrap_or(false)
 {
 path.parent()
 .ok_or_else(|| "Cannot determine store directory from file path".to_string())?
 .to_path_buf()
 } else {
 path
 };

 // Validate the store directory exists and has the expected structure
 let metadata_path = store_dir.join("metadata.redb");
 if !metadata_path.exists() {
 return Err(format!(
 "Invalid store directory: metadata.redb not found in {}",
 store_dir.display()
 ));
 }

 // Check that a workspace with this store path doesn't already exist
 let store_dir_str = store_dir.to_string_lossy().to_string();
 {
 let config = state.workspaces.lock().map_err(|e| e.to_string())?;
 if config
 .workspaces
 .iter()
 .any(|w| w.store_path == store_dir_str)
 {
 return Err("A workspace with this store path already exists.".to_string());
 }
 }

 // Open the store to compute stats
 let store = Store::open(&store_dir).map_err(|e| e.to_string())?;
 let (total_files, total_dirs, unique_blobs, duplicate_files, total_original_bytes, total_stored_bytes) =
 store.compute_stats().map_err(|e| e.to_string())?;

 let now = std::time::SystemTime::now()
 .duration_since(std::time::UNIX_EPOCH)
 .unwrap_or_default()
 .as_secs();

 let ws = Workspace {
 id: workspace::generate_id(),
 label,
 tags: vec!["imported".to_string()],
 store_path: store_dir_str,
 created_at: now,
 stats: workspace::WorkspaceStats {
 total_files,
 total_dirs,
 unique_blobs,
 duplicate_files,
 total_original_bytes,
 total_stored_bytes,
 scans_count: 0,
 last_scan_at: 0,
 },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn custom_rule(id: &str, label: &str, pattern: &str) -> CustomScanRule {
        CustomScanRule {
            id: id.to_string(),
            label: label.to_string(),
            pattern: pattern.to_string(),
            action: workspace::CustomScanRuleAction::Ignore,
            enabled: true,
        }
    }

    #[test]
    fn merge_custom_scan_rules_replaces_existing_rules_by_id_and_appends_new_rules() {
        let mut config = WorkspacesConfig {
            workspaces: Vec::new(),
            active_workspace_id: None,
            custom_scan_rules: vec![
                custom_rule("keep", "Keep", "keep"),
                custom_rule("replace", "Old", "old"),
            ],
        };

        merge_custom_scan_rules(
            &mut config,
            vec![
                custom_rule("replace", "New", "new"),
                custom_rule("append", "Append", "append"),
            ],
        );

        assert_eq!(config.custom_scan_rules.len(), 3);
        assert_eq!(config.custom_scan_rules[0].id, "keep");
        assert_eq!(config.custom_scan_rules[1].id, "replace");
        assert_eq!(config.custom_scan_rules[1].label, "New");
        assert_eq!(config.custom_scan_rules[1].pattern, "new");
        assert_eq!(config.custom_scan_rules[2].id, "append");
    }

    #[test]
    fn transactional_config_update_restores_previous_rules_when_save_fails() {
        let config_dir = std::env::temp_dir().join(format!(
            "dedup-config-dir-{}",
            workspace::generate_id()
        ));
        std::fs::create_dir_all(&config_dir).expect("test config directory should be created");

        let state = AppState::new(config_dir.clone());
        {
            let mut config = state.workspaces.lock().expect("workspace lock should be available");
            config.custom_scan_rules = vec![custom_rule("old", "Old", "old")];
        }

        let result = state.update_config_transactionally(|config| {
            config.custom_scan_rules = vec![custom_rule("new", "New", "new")];
            Ok(())
        });

        assert!(result.is_err());
        let config = state.workspaces.lock().expect("workspace lock should be available");
        assert_eq!(config.custom_scan_rules.len(), 1);
        assert_eq!(config.custom_scan_rules[0].id, "old");

        std::fs::remove_dir_all(config_dir).expect("test config directory should be removed");
    }
}
