use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::cid as cid_util;
use crate::content_store::ContentStore;
use crate::metadata::MetadataDb;
use crate::types::{DirMetadata, FileMetadata, ScanStats};

/// Scan a source directory and replicate it into a content-addressed store.
///
/// - Files are hashed (BLAKE3 → CIDv1), LZ4-compressed, and stored as blobs.
/// - Metadata (paths, sizes, timestamps) is recorded in the redb database.
/// - Duplicate files (same content) are stored only once.
pub fn scan_directory(
    source: &Path,
    content_store: &ContentStore,
    metadata_db: &MetadataDb,
) -> Result<ScanStats> {
    let source = source
        .canonicalize()
        .with_context(|| format!("source directory not found: {}", source.display()))?;

    let mut stats = ScanStats::new();

    for entry in WalkDir::new(&source).follow_links(false) {
        let entry = entry.context("failed to read directory entry")?;
        let abs_path = entry.path();

        // Compute virtual path relative to source root
        let relative = abs_path
            .strip_prefix(&source)
            .context("failed to compute relative path")?;

        // Normalize to forward-slash virtual path
        let virtual_path = if relative.as_os_str().is_empty() {
            "/".to_string()
        } else {
            format!("/{}", relative.to_string_lossy().replace('\\', "/"))
        };

        let fs_meta = fs::metadata(abs_path)
            .with_context(|| format!("failed to read metadata: {}", abs_path.display()))?;

        if fs_meta.is_dir() {
            // Skip the root directory itself
            if virtual_path == "/" {
                continue;
            }

            let modified = fs_meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let dir_meta = DirMetadata {
                child_count: 0, // Will be approximate; updated below is optional
                modified,
            };
            metadata_db
                .insert_dir(&virtual_path, &dir_meta)
                .with_context(|| format!("failed to insert dir: {virtual_path}"))?;

            stats.total_dirs += 1;
        } else if fs_meta.is_file() {
            let data = fs::read(abs_path)
                .with_context(|| format!("failed to read file: {}", abs_path.display()))?;

            let cid = cid_util::compute_cid(&data);
            let cid_str = cid_util::cid_to_string(&cid);

            let was_new = !content_store.exists(&cid);
            let compressed_size = content_store
                .store(&cid, &data)
                .with_context(|| format!("failed to store blob for: {virtual_path}"))?;

            if was_new {
                stats.unique_blobs += 1;
                stats.total_stored_bytes += compressed_size;
            } else {
                stats.duplicate_files += 1;
            }

            let modified = fs_meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let created = fs_meta
                .created()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            #[cfg(unix)]
            let permissions = {
                use std::os::unix::fs::PermissionsExt;
                fs_meta.permissions().mode()
            };
            #[cfg(not(unix))]
            let permissions = 0o644u32;

            let file_meta = FileMetadata {
                cid: cid_util::cid_to_bytes(&cid),
                original_size: data.len() as u64,
                compressed_size,
                modified,
                created,
                permissions,
            };

            metadata_db
                .insert_file(&virtual_path, &file_meta, &cid_str)
                .with_context(|| format!("failed to insert metadata for: {virtual_path}"))?;

            stats.total_files += 1;
            stats.total_original_bytes += data.len() as u64;
        }
        // Skip symlinks, special files, etc.
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_store() -> (TempDir, TempDir, ContentStore, MetadataDb) {
        let source_dir = TempDir::new().unwrap();
        let store_dir = TempDir::new().unwrap();

        let content_store = ContentStore::open(store_dir.path()).unwrap();
        let db_path = store_dir.path().join("metadata.redb");
        let metadata_db = MetadataDb::open(&db_path).unwrap();

        (source_dir, store_dir, content_store, metadata_db)
    }

    #[test]
    fn scan_simple_directory() {
        let (source_dir, _store_dir, content_store, metadata_db) = setup_test_store();

        // Create test files
        fs::write(source_dir.path().join("hello.txt"), b"hello world").unwrap();
        fs::write(source_dir.path().join("bye.txt"), b"goodbye world").unwrap();
        fs::create_dir(source_dir.path().join("subdir")).unwrap();
        fs::write(
            source_dir.path().join("subdir/nested.txt"),
            b"nested content",
        )
        .unwrap();

        let stats = scan_directory(source_dir.path(), &content_store, &metadata_db).unwrap();

        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.total_dirs, 1);
        assert_eq!(stats.unique_blobs, 3);
        assert_eq!(stats.duplicate_files, 0);
    }

    #[test]
    fn scan_detects_duplicates() {
        let (source_dir, _store_dir, content_store, metadata_db) = setup_test_store();

        // Same content in two different files
        fs::write(source_dir.path().join("file1.txt"), b"identical").unwrap();
        fs::write(source_dir.path().join("file2.txt"), b"identical").unwrap();
        fs::write(source_dir.path().join("unique.txt"), b"different").unwrap();

        let stats = scan_directory(source_dir.path(), &content_store, &metadata_db).unwrap();

        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.unique_blobs, 2);
        assert_eq!(stats.duplicate_files, 1);
    }

    #[test]
    fn scan_metadata_queryable() {
        let (source_dir, _store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("test.txt"), b"test content").unwrap();

        scan_directory(source_dir.path(), &content_store, &metadata_db).unwrap();

        // Should be able to resolve the file
        let meta = metadata_db.get_file("/test.txt").unwrap();
        assert!(meta.is_some());
        let meta = meta.unwrap();
        assert_eq!(meta.original_size, 12); // "test content".len()
    }
}
