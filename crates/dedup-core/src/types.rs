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
    /// Name of the file currently being processed.
    pub current_file: String,
}
