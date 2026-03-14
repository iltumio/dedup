use std::path::Path;

use anyhow::{Context, Result};
use redb::{
    Database, MultimapTableDefinition, ReadableMultimapTable, ReadableTable, TableDefinition,
};

use crate::types::{DirEntry, DirMetadata, ExtensionStats, FileMetadata};

/// path (e.g. "/vacation/img1.jpg") → bincode-serialized FileMetadata
const PATHS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("paths");

/// CID string → all virtual paths with that content (dedup index)
const CID_PATHS_TABLE: MultimapTableDefinition<&str, &str> =
    MultimapTableDefinition::new("cid_paths");

/// Directory path → bincode-serialized DirMetadata
const DIRS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("dirs");

/// Wrapper around a redb database for filesystem metadata.
pub struct MetadataDb {
    db: Database,
}

impl MetadataDb {
    /// Open or create the metadata database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let db = Database::create(path)
            .with_context(|| format!("failed to open metadata db: {}", path.display()))?;

        // Create tables eagerly so reads never fail on missing tables.
        let write_txn = db.begin_write()?;
        {
            let _paths = write_txn.open_table(PATHS_TABLE)?;
            let _cid_paths = write_txn.open_multimap_table(CID_PATHS_TABLE)?;
            let _dirs = write_txn.open_table(DIRS_TABLE)?;
        }
        write_txn.commit()?;

        Ok(Self { db })
    }

    /// Insert or update file metadata for a virtual path.
    pub fn insert_file(&self, path: &str, meta: &FileMetadata, cid_str: &str) -> Result<()> {
        let encoded = bincode::serialize(meta).context("failed to serialize FileMetadata")?;

        let write_txn = self.db.begin_write()?;
        {
            let mut paths = write_txn.open_table(PATHS_TABLE)?;
            paths.insert(path, encoded.as_slice())?;

            let mut cid_paths = write_txn.open_multimap_table(CID_PATHS_TABLE)?;
            cid_paths.insert(cid_str, path)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Insert or update directory metadata for a virtual path.
    pub fn insert_dir(&self, path: &str, meta: &DirMetadata) -> Result<()> {
        let encoded = bincode::serialize(meta).context("failed to serialize DirMetadata")?;

        let write_txn = self.db.begin_write()?;
        {
            let mut dirs = write_txn.open_table(DIRS_TABLE)?;
            dirs.insert(path, encoded.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Resolve a file path to its metadata.
    pub fn get_file(&self, path: &str) -> Result<Option<FileMetadata>> {
        let read_txn = self.db.begin_read()?;
        let paths = read_txn.open_table(PATHS_TABLE)?;

        match paths.get(path)? {
            Some(guard) => {
                let bytes = guard.value();
                let meta: FileMetadata =
                    bincode::deserialize(bytes).context("failed to deserialize FileMetadata")?;
                Ok(Some(meta))
            }
            None => Ok(None),
        }
    }

    /// List immediate children of a directory.
    ///
    /// Uses prefix range scan on the paths table and dirs table to find
    /// entries whose path starts with `dir_path/`.
    pub fn list_dir(&self, dir_path: &str) -> Result<Vec<DirEntry>> {
        let prefix = if dir_path == "/" || dir_path.is_empty() {
            "/".to_string()
        } else {
            format!("{}/", dir_path.trim_end_matches('/'))
        };

        let read_txn = self.db.begin_read()?;
        let paths_table = read_txn.open_table(PATHS_TABLE)?;
        let dirs_table = read_txn.open_table(DIRS_TABLE)?;

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Scan files
        let range = paths_table.range::<&str>(prefix.as_str()..)?;
        for item in range {
            let (key, value) = item?;
            let key_str = key.value();

            if !key_str.starts_with(&prefix) {
                break;
            }

            // Extract the immediate child name
            let remainder = &key_str[prefix.len()..];
            // Skip if this is a deeper nested entry
            if let Some(slash_pos) = remainder.find('/') {
                // This is a file inside a subdirectory — record the subdir
                let dir_name = &remainder[..slash_pos];
                if seen.insert(dir_name.to_string()) {
                    entries.push(DirEntry {
                        name: dir_name.to_string(),
                        is_dir: true,
                        size: 0,
                        modified: 0,
                    });
                }
            } else if !remainder.is_empty() {
                // Direct child file
                let meta: FileMetadata = bincode::deserialize(value.value())
                    .context("failed to deserialize FileMetadata")?;
                if seen.insert(remainder.to_string()) {
                    entries.push(DirEntry {
                        name: remainder.to_string(),
                        is_dir: false,
                        size: meta.original_size,
                        modified: meta.modified,
                    });
                }
            }
        }

        // Also scan dirs table for directories that might have no files yet
        let dir_range = dirs_table.range::<&str>(prefix.as_str()..)?;
        for item in dir_range {
            let (key, value) = item?;
            let key_str = key.value();

            if !key_str.starts_with(&prefix) {
                break;
            }

            let remainder = &key_str[prefix.len()..];
            // Only immediate children (no further slashes)
            if !remainder.contains('/')
                && !remainder.is_empty()
                && seen.insert(remainder.to_string())
            {
                let dir_meta: DirMetadata = bincode::deserialize(value.value())
                    .context("failed to deserialize DirMetadata")?;
                entries.push(DirEntry {
                    name: remainder.to_string(),
                    is_dir: true,
                    size: 0,
                    modified: dir_meta.modified,
                });
            }
        }

        entries.sort_by(|a, b| {
            // Directories first, then alphabetical
            b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name))
        });

        Ok(entries)
    }

    /// Find all virtual paths that share the same content (by CID string).
    pub fn find_duplicates(&self, cid_str: &str) -> Result<Vec<String>> {
        let read_txn = self.db.begin_read()?;
        let cid_paths = read_txn.open_multimap_table(CID_PATHS_TABLE)?;

        let mut paths = Vec::new();
        let iter = cid_paths.get(cid_str)?;
        for item in iter {
            let value = item?;
            paths.push(value.value().to_string());
        }
        Ok(paths)
    }

    /// Find all CIDs that have more than one path associated (all duplicate groups).
    pub fn find_all_duplicates(&self) -> Result<Vec<(String, Vec<String>)>> {
        let read_txn = self.db.begin_read()?;
        let cid_paths = read_txn.open_multimap_table(CID_PATHS_TABLE)?;

        let mut groups = Vec::new();
        let iter = cid_paths.iter()?;

        for item in iter {
            let (key, values) = item?;
            let cid_str = key.value().to_string();

            let mut paths = Vec::new();
            for val_item in values {
                let val = val_item?;
                paths.push(val.value().to_string());
            }

            if paths.len() > 1 {
                groups.push((cid_str, paths));
            }
        }

        Ok(groups)
    }

    /// Compute per-extension statistics across all files in the store.
    ///
    /// For each extension, computes total files, duplicate count,
    /// duplicate percentage, total bytes, stored bytes, and bytes saved.
    pub fn extension_stats(&self) -> Result<Vec<ExtensionStats>> {
        use std::collections::HashMap;

        let read_txn = self.db.begin_read()?;
        let paths_table = read_txn.open_table(PATHS_TABLE)?;
        let cid_paths = read_txn.open_multimap_table(CID_PATHS_TABLE)?;

        // First pass: build a CID → count map for detecting duplicates
        let mut cid_count: HashMap<String, u64> = HashMap::new();
        {
            let iter = cid_paths.iter()?;
            for item in iter {
                let (key, values) = item?;
                let cid_str = key.value().to_string();
                let mut count = 0u64;
                for v in values {
                    v?;
                    count += 1;
                }
                cid_count.insert(cid_str, count);
            }
        }

        // Per-extension accumulator
        struct ExtAcc {
            total_files: u64,
            duplicate_files: u64,
            total_original_bytes: u64,
            total_stored_bytes: u64,
        }

        let mut ext_map: HashMap<String, ExtAcc> = HashMap::new();

        // Second pass: iterate all files
        let range = paths_table.iter()?;
        for item in range {
            let item = item?;
            let path: &str = item.0.value();
            let meta: FileMetadata = bincode::deserialize(item.1.value())
                .context("failed to deserialize FileMetadata")?;

            // Extract extension
            let ext: String = path
                .rsplit('/')
                .next()
                .and_then(|name: &str| {
                    let dot = name.rfind('.')?;
                    Some(name[dot + 1..].to_lowercase())
                })
                .unwrap_or_default();

            // Check if this file's CID has duplicates
            let cid_str = crate::cid::cid_to_string(
                &crate::cid::cid_from_bytes(&meta.cid)?,
            );
            let is_dup = cid_count.get(&cid_str).copied().unwrap_or(1) > 1;

            let acc = ext_map.entry(ext).or_insert(ExtAcc {
                total_files: 0,
                duplicate_files: 0,
                total_original_bytes: 0,
                total_stored_bytes: 0,
            });

            acc.total_files += 1;
            acc.total_original_bytes += meta.original_size;
            acc.total_stored_bytes += meta.compressed_size;
            if is_dup {
                acc.duplicate_files += 1;
            }
        }

        // Convert to result vec
        let mut result: Vec<ExtensionStats> = ext_map
            .into_iter()
            .map(|(ext, acc)| {
                let dup_pct = if acc.total_files > 0 {
                    (acc.duplicate_files as f64 / acc.total_files as f64) * 100.0
                } else {
                    0.0
                };
                ExtensionStats {
                    extension: if ext.is_empty() { "(no ext)".into() } else { ext },
                    total_files: acc.total_files,
                    duplicate_files: acc.duplicate_files,
                    duplicate_pct: (dup_pct * 10.0).round() / 10.0,
                    total_original_bytes: acc.total_original_bytes,
                    total_stored_bytes: acc.total_stored_bytes,
                    bytes_saved: acc.total_original_bytes.saturating_sub(acc.total_stored_bytes),
                }
            })
            .collect();

        // Sort by bytes_saved descending (biggest savers first)
        result.sort_by(|a, b| b.bytes_saved.cmp(&a.bytes_saved));

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_db() -> (TempDir, MetadataDb) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.redb");
        let db = MetadataDb::open(&db_path).unwrap();
        (tmp, db)
    }

    fn sample_meta(cid_bytes: &[u8]) -> FileMetadata {
        FileMetadata {
            cid: cid_bytes.to_vec(),
            original_size: 1024,
            compressed_size: 512,
            modified: 1700000000,
            created: 1700000000,
            permissions: 0o644,
        }
    }

    #[test]
    fn insert_and_get_file() {
        let (_tmp, db) = test_db();
        let meta = sample_meta(b"fakecid");
        db.insert_file("/docs/readme.txt", &meta, "bafk_fake_cid")
            .unwrap();

        let retrieved = db.get_file("/docs/readme.txt").unwrap().unwrap();
        assert_eq!(retrieved.original_size, 1024);
        assert_eq!(retrieved.cid, b"fakecid");
    }

    #[test]
    fn list_dir_returns_children() {
        let (_tmp, db) = test_db();
        let meta = sample_meta(b"cid1");
        db.insert_file("/docs/readme.txt", &meta, "cid1").unwrap();
        db.insert_file("/docs/license.md", &meta, "cid1").unwrap();
        db.insert_file("/docs/sub/deep.txt", &meta, "cid1").unwrap();
        db.insert_file("/other/file.txt", &meta, "cid1").unwrap();

        let entries = db.list_dir("/docs").unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

        assert!(names.contains(&"readme.txt"));
        assert!(names.contains(&"license.md"));
        assert!(names.contains(&"sub")); // subdirectory
        assert!(!names.contains(&"file.txt")); // different dir
    }

    #[test]
    fn find_duplicates_works() {
        let (_tmp, db) = test_db();
        let meta = sample_meta(b"same_cid");
        db.insert_file("/a/file1.txt", &meta, "same_cid_str")
            .unwrap();
        db.insert_file("/b/file2.txt", &meta, "same_cid_str")
            .unwrap();

        let dups = db.find_duplicates("same_cid_str").unwrap();
        assert_eq!(dups.len(), 2);
        assert!(dups.contains(&"/a/file1.txt".to_string()));
        assert!(dups.contains(&"/b/file2.txt".to_string()));
    }

    #[test]
    fn get_missing_file_returns_none() {
        let (_tmp, db) = test_db();
        let result = db.get_file("/nonexistent").unwrap();
        assert!(result.is_none());
    }
}
