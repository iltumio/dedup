use serde::{Deserialize, Serialize};

/// Metadata stored for each file in the content-addressed store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// CIDv1 bytes identifying the content.
    pub cid: Vec<u8>,
    /// Original uncompressed file size in bytes.
    pub original_size: u64,
    /// Compressed blob size on disk in bytes.
    pub compressed_size: u64,
    /// Last modification time as unix timestamp (seconds).
    pub modified: i64,
    /// Creation time as unix timestamp (seconds).
    pub created: i64,
    /// Unix file permissions (mode bits).
    pub permissions: u32,
}

/// Metadata stored for each directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirMetadata {
    /// Number of immediate children (files + subdirs).
    pub child_count: u64,
    /// Latest modification time among children.
    pub modified: i64,
}

/// A single entry returned when listing a directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    /// Entry name (not full path).
    pub name: String,
    /// Whether this entry is a directory.
    pub is_dir: bool,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Last modification time as unix timestamp.
    pub modified: i64,
}

/// Statistics returned after a scan operation completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanStats {
    /// Total files discovered during scan.
    pub total_files: u64,
    /// Total directories discovered during scan.
    pub total_dirs: u64,
    /// Number of unique content blobs stored (after dedup).
    pub unique_blobs: u64,
    /// Number of duplicate files detected.
    pub duplicate_files: u64,
    /// Total bytes before deduplication + compression.
    pub total_original_bytes: u64,
    /// Total bytes after deduplication + compression.
    pub total_stored_bytes: u64,
    /// Number of files skipped due to read errors.
    pub skipped_files: u64,
    /// Path to the error log file (if any errors occurred).
    pub errors_log_path: Option<String>,
}

impl ScanStats {
    pub fn new() -> Self {
        Self {
            total_files: 0,
            total_dirs: 0,
            unique_blobs: 0,
            duplicate_files: 0,
            total_original_bytes: 0,
            total_stored_bytes: 0,
            skipped_files: 0,
            errors_log_path: None,
        }
    }
}

impl Default for ScanStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Progress information emitted during a scan operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    /// Number of files processed so far.
    pub files_processed: u64,
    /// Number of directories processed so far.
    pub dirs_processed: u64,
    /// Total bytes of original content processed.
    pub bytes_processed: u64,
    /// Total bytes stored on disk so far (compressed, deduplicated).
    pub bytes_stored: u64,
    /// Number of duplicate files found so far.
    pub duplicates_found: u64,
    /// Number of files skipped due to errors so far.
    pub skipped_files: u64,
    /// Name of the file currently being processed.
    pub current_file: String,
}

/// Action to apply when a scan rule matches a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanRuleAction {
    /// Skip matched files and directories.
    Ignore,
    /// Store a matched directory as one deterministic `.tar` file.
    Archive,
}

/// Built-in scan rule presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinScanPreset {
    Git,
    RustTarget,
    NodeModules,
    PythonVenv,
}

/// A regex-based scan rule matched against the full scan-relative path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanRule {
    pub pattern: String,
    pub action: ScanRuleAction,
}

impl ScanRule {
    pub fn new(pattern: impl Into<String>, action: ScanRuleAction) -> Self {
        Self {
            pattern: pattern.into(),
            action,
        }
    }

    pub fn builtin(preset: BuiltinScanPreset) -> Self {
        match preset {
            BuiltinScanPreset::Git => Self::new(r"(^|/)\.git$", ScanRuleAction::Archive),
            BuiltinScanPreset::RustTarget => Self::new(r"(^|/)target$", ScanRuleAction::Ignore),
            BuiltinScanPreset::NodeModules => {
                Self::new(r"(^|/)node_modules$", ScanRuleAction::Ignore)
            }
            BuiltinScanPreset::PythonVenv => {
                Self::new(r"(^|/)(\.venv|venv)$", ScanRuleAction::Ignore)
            }
        }
    }
}

/// Options controlling scan behavior.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ScanOptions {
    /// Compatibility flag for storing each `.git` directory as one archive blob.
    pub bundle_git_dirs: bool,
    /// Ordered regex rules. First match wins.
    pub rules: Vec<ScanRule>,
}

/// Per-extension statistics for analytics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionStats {
    /// File extension (lowercase, without dot). Empty string for no extension.
    pub extension: String,
    /// Total number of files with this extension.
    pub total_files: u64,
    /// Number of files that are duplicates (share CID with another file).
    pub duplicate_files: u64,
    /// Percentage of files with this extension that are duplicates.
    pub duplicate_pct: f64,
    /// Total original bytes across all files with this extension.
    pub total_original_bytes: u64,
    /// Total stored bytes (compressed, deduplicated) for this extension.
    pub total_stored_bytes: u64,
    /// Bytes saved by deduplication + compression for this extension.
    pub bytes_saved: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::value::{Error as DeError, MapDeserializer, StrDeserializer};
    use serde::Deserialize;

    #[test]
    fn scan_options_deserializes_missing_rules_as_default() {
        let options = ScanOptions::deserialize(MapDeserializer::<_, DeError>::new(
            [("bundle_git_dirs", true)].into_iter(),
        ))
        .unwrap();

        assert!(options.bundle_git_dirs);
        assert!(options.rules.is_empty());
    }

    #[test]
    fn scan_options_deserializes_missing_bundle_git_dirs_as_default() {
        let options = ScanOptions::deserialize(MapDeserializer::<_, DeError>::new(
            [("rules", Vec::<bool>::new())].into_iter(),
        ))
        .unwrap();

        assert!(!options.bundle_git_dirs);
        assert!(options.rules.is_empty());
    }

    #[test]
    fn scan_rule_enums_deserialize_public_snake_case_names() {
        assert_eq!(
            ScanRuleAction::deserialize(StrDeserializer::<DeError>::new("archive")).unwrap(),
            ScanRuleAction::Archive
        );
        assert_eq!(
            BuiltinScanPreset::deserialize(StrDeserializer::<DeError>::new("rust_target")).unwrap(),
            BuiltinScanPreset::RustTarget
        );
        assert_eq!(
            BuiltinScanPreset::deserialize(StrDeserializer::<DeError>::new("python_venv")).unwrap(),
            BuiltinScanPreset::PythonVenv
        );
    }
}
