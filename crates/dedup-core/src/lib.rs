//! dedup-core: Content-addressed storage with virtual filesystem metadata.
//!
//! This library provides:
//! - File scanning and hashing (BLAKE3 → CIDv1)
//! - LZ4-compressed content-addressed blob storage
//! - redb-backed metadata database for virtual directory trees
//! - Duplicate file detection via CID multimap index

mod blob_store;
pub mod cid;
pub mod content_store;
pub mod metadata;
pub mod migration;
pub mod scanner;
pub mod types;

pub use content_store::{ChunkingProfile, ContentStore, StorageStatsPhase, StorageStatsProgress};
pub use metadata::MetadataDb;
pub use types::{
    BuiltinScanPreset, DirEntry, DirMetadata, ExtensionStats, FileMetadata, ScanOptions,
    ScanProgress, ScanRule, ScanRuleAction, ScanStats,
};

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Storage layout chosen at creation and detected automatically on reopening.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageFormat {
    Legacy,
    #[default]
    Fastcdc,
}

/// A complete dedup store combining content storage and metadata.
pub struct Store {
    pub content: ContentStore,
    pub metadata: MetadataDb,
    root: PathBuf,
}

impl Store {
    /// Create an archive in a new or empty directory with an explicit format.
    /// Existing archives must be opened, or migrated to another directory.
    pub fn create(root: &Path, format: StorageFormat) -> Result<Self> {
        if root.try_exists()? {
            anyhow::ensure!(root.is_dir(), "archive location must be a directory");
            anyhow::ensure!(
                std::fs::read_dir(root)?.next().is_none(),
                "archive location must be empty; open an existing archive instead"
            );
        }
        std::fs::create_dir_all(root)?;
        let content = match format {
            StorageFormat::Fastcdc => {
                ContentStore::open_with_profile(root, ChunkingProfile::default())?
            }
            StorageFormat::Legacy => {
                // The v1 layout has no content.redb. Its blobs directory makes
                // the format recognizable before any file has been archived.
                std::fs::create_dir(root.join("blobs"))?;
                ContentStore::open(root)?
            }
        };
        let metadata = MetadataDb::open(&root.join("metadata.redb"))?;
        Ok(Self {
            content,
            metadata,
            root: root.to_path_buf(),
        })
    }

    /// Open or create a dedup store at the given root directory.
    ///
    /// Creates the following structure:
    /// ```text
    /// <root>/
    ///   blobs/        — LZ4-compressed content chunks (whole files in v1)
    ///   content.redb  — FastCDC format/profile and ordered file manifests
    ///   metadata.redb — virtual filesystem metadata
    /// ```
    pub fn open(root: &Path) -> Result<Self> {
        migration::ensure_complete(root)?;
        Self::open_impl(root, false)
    }

    /// Open a pre-existing archive, without creating a store on a typo.
    pub fn open_existing(root: &Path) -> Result<Self> {
        migration::ensure_complete(root)?;
        anyhow::ensure!(
            root.join("metadata.redb").is_file(),
            "source store does not exist"
        );
        anyhow::ensure!(
            root.join("blobs").is_dir(),
            "source store has no blobs directory"
        );
        Self::open_impl(root, true)
    }

    pub(crate) fn open_impl(root: &Path, existing: bool) -> Result<Self> {
        std::fs::create_dir_all(root)
            .with_context(|| format!("failed to create store root: {}", root.display()))?;

        let content = ContentStore::open(root)?;
        let db_path = root.join("metadata.redb");
        let metadata = if existing {
            MetadataDb::open_existing(&db_path)?
        } else {
            MetadataDb::open(&db_path)?
        };

        Ok(Self {
            content,
            metadata,
            root: root.to_path_buf(),
        })
    }

    /// Scan a source directory and replicate it into this store under `/`.
    pub fn scan(&self, source: &Path) -> Result<ScanStats> {
        scanner::scan_directory(source, &self.root, &self.content, &self.metadata)
    }

    /// Scan a source directory into a target virtual path (incremental).
    ///
    /// Existing entries in the store are preserved. New entries are added
    /// under `target_path`. The `on_progress` callback is invoked after
    /// each file is processed.
    pub fn scan_into<F>(
        &self,
        source: &Path,
        target_path: &str,
        on_progress: F,
    ) -> Result<ScanStats>
    where
        F: Fn(&types::ScanProgress),
    {
        scanner::scan_directory_into(
            source,
            target_path,
            &self.root,
            &self.content,
            &self.metadata,
            on_progress,
        )
    }

    /// Scan a source directory into a target virtual path with explicit options.
    pub fn scan_into_with_options<F>(
        &self,
        source: &Path,
        target_path: &str,
        options: ScanOptions,
        on_progress: F,
    ) -> Result<ScanStats>
    where
        F: Fn(&types::ScanProgress),
    {
        scanner::scan_directory_into_with_options(
            source,
            target_path,
            &self.root,
            &self.content,
            &self.metadata,
            options,
            on_progress,
        )
    }

    /// Scan a source directory into a target virtual path with cooperative cancellation.
    pub fn scan_into_with_cancellation<F, C>(
        &self,
        source: &Path,
        target_path: &str,
        on_progress: F,
        should_cancel: C,
    ) -> Result<ScanStats>
    where
        F: Fn(&types::ScanProgress),
        C: Fn() -> bool,
    {
        self.scan_into_with_options_and_cancellation(
            source,
            target_path,
            ScanOptions::default(),
            on_progress,
            should_cancel,
        )
    }

    /// Scan a source directory into a target virtual path with options and cooperative cancellation.
    pub fn scan_into_with_options_and_cancellation<F, C>(
        &self,
        source: &Path,
        target_path: &str,
        options: ScanOptions,
        on_progress: F,
        should_cancel: C,
    ) -> Result<ScanStats>
    where
        F: Fn(&types::ScanProgress),
        C: Fn() -> bool,
    {
        scanner::scan_directory_into_with_options_and_cancellation(
            source,
            target_path,
            &self.root,
            &self.content,
            &self.metadata,
            options,
            on_progress,
            should_cancel,
        )
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
    /// Resolves path → whole-file CID → verified reconstructed content.
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

    /// Compute per-extension statistics across all files. Shared payload bytes
    /// are split equally among referencing extensions, with deterministic rounding.
    pub fn extension_stats(&self) -> Result<Vec<ExtensionStats>> {
        use std::collections::{BTreeMap, BTreeSet};
        let mut stats = self.metadata.extension_stats()?;
        let snapshot = self.metadata.snapshot()?;
        let mut payloads: BTreeMap<String, (u64, BTreeSet<String>)> = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for (path, meta) in snapshot.files {
            let ext = path
                .rsplit('/')
                .next()
                .and_then(|name| name.rsplit_once('.'))
                .map(|(_, ext)| ext.to_lowercase())
                .filter(|ext| !ext.is_empty())
                .unwrap_or_else(|| "(no ext)".to_owned());
            let cid = cid::cid_from_bytes(&meta.cid)?;
            if !seen.insert((ext.clone(), cid)) {
                continue;
            }
            for (key, bytes) in self.content.storage_entries(&cid)? {
                payloads
                    .entry(key)
                    .or_insert_with(|| (bytes, BTreeSet::new()))
                    .1
                    .insert(ext.clone());
            }
        }
        let mut allocated: BTreeMap<String, u64> = BTreeMap::new();
        for (_, (bytes, owners)) in payloads {
            let count = owners.len() as u64;
            for (index, ext) in owners.into_iter().enumerate() {
                *allocated.entry(ext).or_default() +=
                    bytes / count + u64::from((index as u64) < bytes % count);
            }
        }
        for row in &mut stats {
            row.total_stored_bytes = allocated.get(&row.extension).copied().unwrap_or(0);
            row.bytes_saved = row
                .total_original_bytes
                .saturating_sub(row.total_stored_bytes);
        }
        Ok(stats)
    }

    /// Compute aggregate statistics for the entire store.
    ///
    /// Returns (total_files, total_dirs, unique_blobs, duplicate_files,
    /// total_original_bytes, total_stored_bytes).
    /// Stored bytes count referenced payloads and manifests once, excluding
    /// database allocation overhead and unreferenced historical content.
    pub fn compute_stats(&self) -> Result<(u64, u64, u64, u64, u64, u64)> {
        self.compute_stats_with_progress(|_| Ok(()))
    }

    /// Count referenced chunks once, reporting manifest and filesystem progress.
    /// Return an error from the callback to cancel the read-only calculation.
    pub fn compute_stats_with_progress(
        &self,
        on_progress: impl FnMut(StorageStatsProgress) -> Result<()>,
    ) -> Result<(u64, u64, u64, u64, u64, u64)> {
        let (files, dirs, unique, duplicates, original, _) = self.metadata.compute_stats()?;
        let cids = self
            .metadata
            .snapshot()?
            .files
            .into_iter()
            .map(|(_, meta)| cid::cid_from_bytes(&meta.cid))
            .collect::<Result<Vec<_>>>()?;
        let stored = self
            .content
            .referenced_size_with_progress(cids, on_progress)?;
        Ok((files, dirs, unique, duplicates, original, stored))
    }
}
