use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomScanRuleAction {
    Ignore,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomScanRule {
    pub id: String,
    pub label: String,
    pub pattern: String,
    pub action: CustomScanRuleAction,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// The workspaces config file format.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspacesConfig {
    pub workspaces: Vec<Workspace>,
    /// ID of the currently active workspace (if any).
    pub active_workspace_id: Option<String>,
    #[serde(default)]
    pub custom_scan_rules: Vec<CustomScanRule>,
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
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let temp_path = temp_config_path(path);

        let result = fs::write(&temp_path, data)
            .map_err(|e| e.to_string())
            .and_then(|_| fs::rename(&temp_path, path).map_err(|e| e.to_string()));

        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }

        result
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

fn temp_config_path(path: &Path) -> PathBuf {
    if let Some(file_name) = path.file_name() {
        let mut temp_file_name = OsString::from(".");
        temp_file_name.push(file_name);
        temp_file_name.push(".tmp");
        path.with_file_name(temp_file_name)
    } else {
        PathBuf::from(".dedup-workspaces.json.tmp")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn custom_rule(id: &str, pattern: &str) -> CustomScanRule {
        CustomScanRule {
            id: id.to_string(),
            label: format!("Rule {id}"),
            pattern: pattern.to_string(),
            action: CustomScanRuleAction::Ignore,
            enabled: true,
        }
    }

    fn temp_config_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dedup-workspace-{label}-{}", generate_id()))
    }

    #[test]
    fn deserializes_missing_custom_scan_rules_as_empty() {
        let config: WorkspacesConfig = serde_json::from_str(
            r#"{
                "workspaces": [],
                "active_workspace_id": null
            }"#,
        )
        .expect("config should deserialize");

        assert!(config.custom_scan_rules.is_empty());
    }

    #[test]
    fn deserializes_custom_scan_rule_defaults_and_actions() {
        let config: WorkspacesConfig = serde_json::from_str(
            r#"{
                "workspaces": [],
                "active_workspace_id": null,
                "custom_scan_rules": [
                    {
                        "id": "ignore-rule",
                        "label": "Ignore temp files",
                        "pattern": "\\.tmp$",
                        "action": "ignore"
                    },
                    {
                        "id": "archive-rule",
                        "label": "Archive logs",
                        "pattern": "\\.log$",
                        "action": "archive",
                        "enabled": false
                    }
                ]
            }"#,
        )
        .expect("config should deserialize");

        let ignore_rule = &config.custom_scan_rules[0];
        assert!(matches!(ignore_rule.action, CustomScanRuleAction::Ignore));
        assert!(ignore_rule.enabled);

        let archive_rule = &config.custom_scan_rules[1];
        assert!(matches!(archive_rule.action, CustomScanRuleAction::Archive));
        assert!(!archive_rule.enabled);
    }

    #[test]
    fn save_writes_valid_config_roundtrip() {
        let config_dir = temp_config_dir("save-roundtrip");
        std::fs::create_dir_all(&config_dir).expect("test config directory should be created");
        let config_path = config_dir.join("dedup-workspaces.json");

        let config = WorkspacesConfig {
            workspaces: Vec::new(),
            active_workspace_id: Some("workspace-1".to_string()),
            custom_scan_rules: vec![custom_rule("rule-1", "\\.tmp$")],
        };

        config.save(&config_path).expect("config should save");

        let loaded = WorkspacesConfig::load(&config_path).expect("config should load");
        assert_eq!(loaded.active_workspace_id.as_deref(), Some("workspace-1"));
        assert_eq!(loaded.custom_scan_rules.len(), 1);
        assert_eq!(loaded.custom_scan_rules[0].id, "rule-1");
        assert!(!config_dir.join(".dedup-workspaces.json.tmp").exists());

        std::fs::remove_dir_all(config_dir).expect("test config directory should be removed");
    }

    #[test]
    fn failed_temp_write_does_not_clobber_existing_config() {
        let config_dir = temp_config_dir("failed-temp-write");
        std::fs::create_dir_all(&config_dir).expect("test config directory should be created");
        let config_path = config_dir.join("dedup-workspaces.json");

        let original = WorkspacesConfig {
            workspaces: Vec::new(),
            active_workspace_id: Some("original".to_string()),
            custom_scan_rules: vec![custom_rule("original-rule", "original")],
        };
        original
            .save(&config_path)
            .expect("original config should save");

        std::fs::create_dir(config_dir.join(".dedup-workspaces.json.tmp"))
            .expect("temp path blocker should be created");

        let replacement = WorkspacesConfig {
            workspaces: Vec::new(),
            active_workspace_id: Some("replacement".to_string()),
            custom_scan_rules: vec![custom_rule("replacement-rule", "replacement")],
        };

        let result = replacement.save(&config_path);

        assert!(result.is_err());
        let loaded = WorkspacesConfig::load(&config_path).expect("config should still load");
        assert_eq!(loaded.active_workspace_id.as_deref(), Some("original"));
        assert_eq!(loaded.custom_scan_rules[0].id, "original-rule");

        std::fs::remove_dir_all(config_dir).expect("test config directory should be removed");
    }
}
