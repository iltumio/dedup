use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::cid as cid_util;
use crate::content_store::ContentStore;
use crate::metadata::MetadataDb;
use crate::types::{DirMetadata, FileMetadata, ScanProgress, ScanStats};

/// Scan a source directory and replicate it into a content-addressed store.
///
/// Files are placed under the virtual root `/`.
/// This is a convenience wrapper around [`scan_directory_into`].
pub fn scan_directory(
    source: &Path,
    store_root: &Path,
    content_store: &ContentStore,
    metadata_db: &MetadataDb,
) -> Result<ScanStats> {
    scan_directory_into(source, "/", store_root, content_store, metadata_db, |_| {})
}

/// Scan a source directory and place its contents under `target_prefix` in
/// the virtual filesystem.
///
/// - If `target_prefix` is `"/"`, files go to `/foo.txt`, `/sub/bar.txt`, etc.
/// - If `target_prefix` is `"/photos/vacation"`, files go to
///   `/photos/vacation/foo.txt`, etc.
/// - Existing entries in the store are preserved (incremental).
/// - Duplicate content is stored only once (deduplicated).
///
/// Files that fail to read (permission denied, I/O errors, etc.) are skipped
/// and their errors are logged to `<store_root>/errors_<vdir_name>.log`.
///
/// The `on_progress` callback is invoked after each file is processed,
/// enabling real-time progress reporting.
pub fn scan_directory_into<F>(
    source: &Path,
    target_prefix: &str,
    store_root: &Path,
    content_store: &ContentStore,
    metadata_db: &MetadataDb,
    on_progress: F,
) -> Result<ScanStats>
where
    F: Fn(&ScanProgress),
{
    let source = source
        .canonicalize()
        .with_context(|| format!("source directory not found: {}", source.display()))?;

    // Normalize target prefix: ensure it starts with / and has no trailing /
    let prefix = if target_prefix == "/" || target_prefix.is_empty() {
        String::new()
    } else {
        let p = target_prefix.trim_end_matches('/');
        if p.starts_with('/') {
            p.to_string()
        } else {
            format!("/{p}")
        }
    };

    // Ensure the target directory itself is registered if it's not root
    if !prefix.is_empty() {
        ensure_parent_dirs(metadata_db, &prefix)?;
    }

    let mut stats = ScanStats::new();

    // Build the error log path from the virtual directory name.
    // e.g. target_prefix="/photos/vacation" → "errors_photos_vacation.log"
    let error_log_name = {
        let sanitized = if prefix.is_empty() {
            "root".to_string()
        } else {
            prefix
                .trim_start_matches('/')
                .replace('/', "_")
                .replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', "_")
        };
        format!("errors_{sanitized}.log")
    };
    let error_log_path = store_root.join(&error_log_name);

    // Lazily-opened error log file handle. Only created if errors occur.
    let mut error_log: Option<fs::File> = None;

    /// Write an error entry to the log file, creating it if needed.
    macro_rules! log_error {
        ($log:expr, $path:expr, $err:expr, $entry_path:expr) => {{
            let file = match $log {
                Some(ref mut f) => f,
                None => {
                    let f = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&$path)
                        .ok();
                    match f {
                        Some(f) => {
                            *$log = Some(f);
                            ($log).as_mut().unwrap()
                        }
                        None => {
                            // Can't even open the error log — just skip silently.
                            return;
                        }
                    }
                }
            };
            let timestamp = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let _ = writeln!(file, "[{timestamp}] {}: {}", $entry_path, $err);
        }};
    }

    for entry in WalkDir::new(&source).follow_links(false) {
        // Error reading the directory entry itself (e.g. permission denied on dir)
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                let entry_path = err
                    .path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                (|| log_error!(&mut error_log, error_log_path, err, entry_path))();
                stats.skipped_files += 1;
                continue;
            }
        };
        let abs_path = entry.path();

        // Compute virtual path relative to source root
        let relative = match abs_path.strip_prefix(&source) {
            Ok(r) => r,
            Err(err) => {
                let display = abs_path.display().to_string();
                (|| log_error!(&mut error_log, error_log_path, err, display))();
                stats.skipped_files += 1;
                continue;
            }
        };

        let rel_str = relative.to_string_lossy().replace('\\', "/");

        // Build the full virtual path under the target prefix
        let virtual_path = if rel_str.is_empty() {
            if prefix.is_empty() {
                continue; // skip root when scanning into /
            } else {
                prefix.clone()
            }
        } else {
            format!("{prefix}/{rel_str}")
        };

        // Read filesystem metadata — skip on error
        let fs_meta = match fs::metadata(abs_path) {
            Ok(m) => m,
            Err(err) => {
                let msg = format!("failed to read metadata: {err}");
                let display = abs_path.display().to_string();
                (|| log_error!(&mut error_log, error_log_path, msg, display))();
                stats.skipped_files += 1;
                continue;
            }
        };

        if fs_meta.is_dir() {
            let modified = extract_mtime(&fs_meta);

            let dir_meta = DirMetadata {
                child_count: 0,
                modified,
            };
            metadata_db
                .insert_dir(&virtual_path, &dir_meta)
                .with_context(|| format!("failed to insert dir: {virtual_path}"))?;

            stats.total_dirs += 1;
        } else if fs_meta.is_file() {
            // Read file content — skip on error
            let data = match fs::read(abs_path) {
                Ok(d) => d,
                Err(err) => {
                    let msg = format!("failed to read file: {err}");
                    let display = abs_path.display().to_string();
                    (|| log_error!(&mut error_log, error_log_path, msg, display))();
                    stats.skipped_files += 1;
                    on_progress(&ScanProgress {
                        files_processed: stats.total_files,
                        dirs_processed: stats.total_dirs,
                        bytes_processed: stats.total_original_bytes,
                        bytes_stored: stats.total_stored_bytes,
                        duplicates_found: stats.duplicate_files,
                        skipped_files: stats.skipped_files,
                        current_file: virtual_path,
                    });
                    continue;
                }
            };

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

            let modified = extract_mtime(&fs_meta);
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

            // Emit progress
            on_progress(&ScanProgress {
                files_processed: stats.total_files,
                dirs_processed: stats.total_dirs,
                bytes_processed: stats.total_original_bytes,
                bytes_stored: stats.total_stored_bytes,
                duplicates_found: stats.duplicate_files,
                skipped_files: stats.skipped_files,
                current_file: virtual_path,
            });
        }
        // Skip symlinks, special files, etc.
    }

    // Set the error log path in stats if any errors were logged
    if error_log.is_some() {
        stats.errors_log_path = Some(error_log_path.to_string_lossy().to_string());
    }

    Ok(stats)
}

/// Ensure all ancestor directories of `path` exist in the metadata db.
/// For example, for "/a/b/c", ensures "/a" and "/a/b" and "/a/b/c" are registered.
fn ensure_parent_dirs(metadata_db: &MetadataDb, path: &str) -> Result<()> {
    let parts: Vec<&str> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let mut current = String::new();
    for part in parts {
        current = format!("{current}/{part}");
        // Only insert if not already present — insert_dir is idempotent (upsert)
        let dir_meta = DirMetadata {
            child_count: 0,
            modified: 0,
        };
        metadata_db.insert_dir(&current, &dir_meta)?;
    }
    Ok(())
}

fn extract_mtime(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("hello.txt"), b"hello world").unwrap();
        fs::write(source_dir.path().join("bye.txt"), b"goodbye world").unwrap();
        fs::create_dir(source_dir.path().join("subdir")).unwrap();
        fs::write(
            source_dir.path().join("subdir/nested.txt"),
            b"nested content",
        )
        .unwrap();

        let stats = scan_directory(
            source_dir.path(),
            store_dir.path(),
            &content_store,
            &metadata_db,
        )
        .unwrap();

        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.total_dirs, 1);
        assert_eq!(stats.unique_blobs, 3);
        assert_eq!(stats.duplicate_files, 0);
        assert_eq!(stats.skipped_files, 0);
    }

    #[test]
    fn scan_detects_duplicates() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("file1.txt"), b"identical").unwrap();
        fs::write(source_dir.path().join("file2.txt"), b"identical").unwrap();
        fs::write(source_dir.path().join("unique.txt"), b"different").unwrap();

        let stats = scan_directory(
            source_dir.path(),
            store_dir.path(),
            &content_store,
            &metadata_db,
        )
        .unwrap();

        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.unique_blobs, 2);
        assert_eq!(stats.duplicate_files, 1);
    }

    #[test]
    fn scan_metadata_queryable() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("test.txt"), b"test content").unwrap();

        scan_directory(
            source_dir.path(),
            store_dir.path(),
            &content_store,
            &metadata_db,
        )
        .unwrap();

        let meta = metadata_db.get_file("/test.txt").unwrap();
        assert!(meta.is_some());
        let meta = meta.unwrap();
        assert_eq!(meta.original_size, 12);
    }

    #[test]
    fn scan_into_subdirectory() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("a.txt"), b"aaa").unwrap();
        fs::write(source_dir.path().join("b.txt"), b"bbb").unwrap();

        let stats = scan_directory_into(
            source_dir.path(),
            "/photos/vacation",
            store_dir.path(),
            &content_store,
            &metadata_db,
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 2);

        // Files should be under the target prefix
        assert!(metadata_db
            .get_file("/photos/vacation/a.txt")
            .unwrap()
            .is_some());
        assert!(metadata_db
            .get_file("/photos/vacation/b.txt")
            .unwrap()
            .is_some());
        // Root-level should not have them
        assert!(metadata_db.get_file("/a.txt").unwrap().is_none());

        // Parent dirs should be created
        let root_entries = metadata_db.list_dir("/").unwrap();
        let names: Vec<&str> = root_entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"photos"));
    }

    #[test]
    fn incremental_scan_preserves_existing() {
        let (source1, store_dir, content_store, metadata_db) = setup_test_store();

        // First scan
        fs::write(source1.path().join("original.txt"), b"original").unwrap();
        scan_directory(
            source1.path(),
            store_dir.path(),
            &content_store,
            &metadata_db,
        )
        .unwrap();

        // Second scan into a subdirectory
        let source2 = TempDir::new().unwrap();
        fs::write(source2.path().join("new.txt"), b"new content").unwrap();
        scan_directory_into(
            source2.path(),
            "/imported",
            store_dir.path(),
            &content_store,
            &metadata_db,
            |_| {},
        )
        .unwrap();

        // Both should be queryable
        assert!(metadata_db.get_file("/original.txt").unwrap().is_some());
        assert!(metadata_db.get_file("/imported/new.txt").unwrap().is_some());
    }

    #[test]
    fn progress_callback_fires() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("a.txt"), b"aaa").unwrap();
        fs::write(source_dir.path().join("b.txt"), b"bbb").unwrap();

        use std::sync::atomic::{AtomicU64, Ordering};
        let count = AtomicU64::new(0);
        scan_directory_into(
            source_dir.path(),
            "/",
            store_dir.path(),
            &content_store,
            &metadata_db,
            |_p| {
                count.fetch_add(1, Ordering::Relaxed);
            },
        )
        .unwrap();

        assert!(count.load(Ordering::Relaxed) >= 2);
    }
}
