//! dedup-core: Content-addressed storage with virtual filesystem metadata.
//!
//! This library provides:
//! - File scanning and hashing (BLAKE3 → CIDv1)
//! - LZ4-compressed content-addressed blob storage
//! - redb-backed metadata database for virtual directory trees
//! - Duplicate file detection via CID multimap index

pub mod cid;
pub mod content_store;
pub mod metadata;
pub mod scanner;
pub mod types;

pub use content_store::ContentStore;
pub use metadata::MetadataDb;
pub use types::{DirEntry, DirMetadata, FileMetadata, ScanProgress, ScanStats};

use std::path::Path;

use anyhow::{Context, Result};

/// A complete dedup store combining content storage and metadata.
pub struct Store {
    pub content: ContentStore,
    pub metadata: MetadataDb,
}

impl Store {
    /// Open or create a dedup store at the given root directory.
    ///
    /// Creates the following structure:
    /// ```text
    /// <root>/
    ///   blobs/        — LZ4-compressed content blobs
    ///   metadata.redb — virtual filesystem metadata
    /// ```
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)
            .with_context(|| format!("failed to create store root: {}", root.display()))?;

        let content = ContentStore::open(root)?;
        let db_path = root.join("metadata.redb");
        let metadata = MetadataDb::open(&db_path)?;

        Ok(Self { content, metadata })
    }

    /// Scan a source directory and replicate it into this store under `/`.
    pub fn scan(&self, source: &Path) -> Result<ScanStats> {
        scanner::scan_directory(source, &self.content, &self.metadata)
    }

    /// Scan a source directory into a target virtual path (incremental).
    ///
    /// Existing entries in the store are preserved. New entries are added
    /// under `target_path`. The `on_progress` callback is invoked after
    /// each file is processed.
    pub fn scan_into<F>(&self, source: &Path, target_path: &str, on_progress: F) -> Result<ScanStats>
    where
        F: Fn(&types::ScanProgress),
    {
        scanner::scan_directory_into(source, target_path, &self.content, &self.metadata, on_progress)
    }

    /// List entries in a virtual directory.
    pub fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        self.metadata.list_dir(path)
    }

    /// Get metadata for a virtual file path.
    pub fn get_file(&self, path: &str) -> Result<Option<FileMetadata>> {
        self.metadata.get_file(path)
    }

    /// Read the content of a file by its virtual path.
    ///
    /// Resolves path → CID → decompressed blob.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let meta = self
            .metadata
            .get_file(path)?
            .with_context(|| format!("file not found: {path}"))?;

        let file_cid = cid::cid_from_bytes(&meta.cid).context("invalid CID in file metadata")?;

        self.content
            .read(&file_cid)
            .with_context(|| format!("failed to read blob for: {path}"))
    }

    /// Find all paths that share the same content as the file at `path`.
    pub fn find_duplicates(&self, path: &str) -> Result<Vec<String>> {
        let meta = self
            .metadata
            .get_file(path)?
            .with_context(|| format!("file not found: {path}"))?;

        let file_cid = cid::cid_from_bytes(&meta.cid)?;
        let cid_str = cid::cid_to_string(&file_cid);

        self.metadata.find_duplicates(&cid_str)
    }

    /// Find all groups of duplicate files in the store.
    pub fn find_all_duplicates(&self) -> Result<Vec<(String, Vec<String>)>> {
        self.metadata.find_all_duplicates()
    }
}
