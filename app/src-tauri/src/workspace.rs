use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Accumulated statistics for a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceStats {
    pub total_files: u64,
    pub total_dirs: u64,
    pub unique_blobs: u64,
    pub duplicate_files: u64,
    pub total_original_bytes: u64,
    pub total_stored_bytes: u64,
    pub scans_count: u64,
    pub last_scan_at: u64,
}

/// A single workspace entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// Unique identifier (UUID-like, generated on creation).
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// User-defined tags for organization.
    pub tags: Vec<String>,
    /// Absolute path to the store directory (contains blobs/ + metadata.redb).
    pub store_path: String,
    /// Unix timestamp of creation.
    pub created_at: u64,
    /// Accumulated scan statistics.
    #[serde(default)]
    pub stats: WorkspaceStats,
}

/// The workspaces config file format.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspacesConfig {
    pub workspaces: Vec<Workspace>,
    /// ID of the currently active workspace (if any).
    pub active_workspace_id: Option<String>,
}

impl WorkspacesConfig {
    /// Load from a JSON file, returning default if file doesn't exist.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&data).map_err(|e| e.to_string())
    }

    /// Save to a JSON file.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| e.to_string())
    }

    /// Find a workspace by ID.
    pub fn find(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    /// Find a workspace by ID (mutable).
    pub fn find_mut(&mut self, id: &str) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|w| w.id == id)
    }

    /// Get the active workspace (if set and exists).
    pub fn active(&self) -> Option<&Workspace> {
        self.active_workspace_id
            .as_deref()
            .and_then(|id| self.find(id))
    }
}

/// Default location for the workspaces config file.
/// Uses the app's config directory: ~/.config/dedup/dedup-workspaces.json (Linux)
pub fn default_config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dedup");
    config_dir.join("dedup-workspaces.json")
}

/// Generate a simple unique ID.
pub fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("ws_{ts:x}")
}
