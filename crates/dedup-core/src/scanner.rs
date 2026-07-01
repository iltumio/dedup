use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{bail, Context, Result};
use regex::Regex;
use walkdir::WalkDir;

use crate::cid as cid_util;
use crate::content_store::ContentStore;
use crate::metadata::MetadataDb;
use crate::types::{
    BuiltinScanPreset, DirMetadata, FileMetadata, ScanOptions, ScanProgress, ScanRule,
    ScanRuleAction, ScanStats,
};

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
    scan_directory_into_with_options(
        source,
        target_prefix,
        store_root,
        content_store,
        metadata_db,
        ScanOptions::default(),
        on_progress,
    )
}

pub fn scan_directory_into_with_options<F>(
    source: &Path,
    target_prefix: &str,
    store_root: &Path,
    content_store: &ContentStore,
    metadata_db: &MetadataDb,
    options: ScanOptions,
    on_progress: F,
) -> Result<ScanStats>
where
    F: Fn(&ScanProgress),
{
    scan_directory_into_with_options_and_cancellation(
        source,
        target_prefix,
        store_root,
        content_store,
        metadata_db,
        options,
        on_progress,
        || false,
    )
}

/// Scan a source directory into a target virtual path with cooperative
/// cancellation between filesystem entries.
pub fn scan_directory_into_with_cancellation<F, C>(
    source: &Path,
    target_prefix: &str,
    store_root: &Path,
    content_store: &ContentStore,
    metadata_db: &MetadataDb,
    on_progress: F,
    should_cancel: C,
) -> Result<ScanStats>
where
    F: Fn(&ScanProgress),
    C: Fn() -> bool,
{
    scan_directory_into_with_options_and_cancellation(
        source,
        target_prefix,
        store_root,
        content_store,
        metadata_db,
        ScanOptions::default(),
        on_progress,
        should_cancel,
    )
}

#[derive(Debug)]
struct CompiledScanRule {
    regex: Regex,
    action: ScanRuleAction,
}

fn compile_scan_rules(options: &ScanOptions) -> Result<Vec<CompiledScanRule>> {
    let mut rules: Vec<ScanRule> = Vec::new();

    if options.bundle_git_dirs {
        rules.push(ScanRule::builtin(BuiltinScanPreset::Git));
    }

    rules.extend(options.rules.clone());

    rules
        .into_iter()
        .map(|rule| {
            let regex = Regex::new(&rule.pattern)
                .with_context(|| format!("invalid scan rule regex: {}", rule.pattern))?;
            Ok(CompiledScanRule {
                regex,
                action: rule.action,
            })
        })
        .collect()
}

fn matching_rule<'a>(
    rules: &'a [CompiledScanRule],
    relative_path: &str,
) -> Option<&'a CompiledScanRule> {
    rules.iter().find(|rule| rule.regex.is_match(relative_path))
}

fn emit_skipped_progress<F>(virtual_path: String, stats: &ScanStats, on_progress: &F)
where
    F: Fn(&ScanProgress),
{
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

pub fn scan_directory_into_with_options_and_cancellation<F, C>(
    source: &Path,
    target_prefix: &str,
    store_root: &Path,
    content_store: &ContentStore,
    metadata_db: &MetadataDb,
    options: ScanOptions,
    on_progress: F,
    should_cancel: C,
) -> Result<ScanStats>
where
    F: Fn(&ScanProgress),
    C: Fn() -> bool,
{
    let source = source
        .canonicalize()
        .with_context(|| format!("source directory not found: {}", source.display()))?;

    let compiled_rules = compile_scan_rules(&options)?;

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

    let mut reserved_git_archive_paths = HashSet::new();
    let mut walker = WalkDir::new(&source).follow_links(false).into_iter();
    while let Some(entry) = walker.next() {
        if should_cancel() {
            bail!("scan cancelled");
        }

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
        let entry_file_type = entry.file_type();

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

        if let Some(rule) = matching_rule(&compiled_rules, &rel_str) {
            match rule.action {
                ScanRuleAction::Ignore => {
                    stats.skipped_files += 1;
                    emit_skipped_progress(virtual_path, &stats, &on_progress);
                    if entry_file_type.is_dir() {
                        walker.skip_current_dir();
                    }
                    continue;
                }
                ScanRuleAction::Archive => {
                    if !entry_file_type.is_dir() {
                        stats.skipped_files += 1;
                        emit_skipped_progress(virtual_path, &stats, &on_progress);
                        continue;
                    }
                }
            }
        }

        if options.bundle_git_dirs
            && (reserved_git_archive_paths.contains(&virtual_path)
                || has_real_sibling_git_dir_for_git_tar_collision(abs_path))
        {
            let msg = "virtual path collision: .git.tar is reserved for bundled .git directory"
                .to_string();
            (|| log_error!(&mut error_log, error_log_path, msg, virtual_path.clone()))();
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
            if entry_file_type.is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }

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

        if options.bundle_git_dirs
            && entry_file_type.is_dir()
            && abs_path
                .file_name()
                .map(|name| name == ".git")
                .unwrap_or(false)
        {
            let archive_virtual_path = format!("{virtual_path}.tar");
            let archive_data = match build_git_directory_archive(abs_path, &should_cancel) {
                Ok(data) => data,
                Err(err) => {
                    if is_scan_cancelled_error(&err) {
                        return Err(err);
                    }

                    let msg = format!("failed to archive .git directory: {err}");
                    (|| {
                        log_error!(
                            &mut error_log,
                            error_log_path,
                            msg,
                            archive_virtual_path.clone()
                        )
                    })();
                    stats.skipped_files += 1;
                    on_progress(&ScanProgress {
                        files_processed: stats.total_files,
                        dirs_processed: stats.total_dirs,
                        bytes_processed: stats.total_original_bytes,
                        bytes_stored: stats.total_stored_bytes,
                        duplicates_found: stats.duplicate_files,
                        skipped_files: stats.skipped_files,
                        current_file: archive_virtual_path,
                    });
                    walker.skip_current_dir();
                    continue;
                }
            };

            let modified = extract_mtime(&fs_meta);
            let created = extract_ctime(&fs_meta);
            store_virtual_file(
                &archive_virtual_path,
                &archive_data,
                modified,
                created,
                0o644,
                content_store,
                metadata_db,
                &mut stats,
                &on_progress,
            )?;
            reserved_git_archive_paths.insert(archive_virtual_path);

            walker.skip_current_dir();
            continue;
        }

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

            let modified = extract_mtime(&fs_meta);
            let created = extract_ctime(&fs_meta);

            #[cfg(unix)]
            let permissions = {
                use std::os::unix::fs::PermissionsExt;
                fs_meta.permissions().mode()
            };
            #[cfg(not(unix))]
            let permissions = 0o644u32;

            store_virtual_file(
                &virtual_path,
                &data,
                modified,
                created,
                permissions,
                content_store,
                metadata_db,
                &mut stats,
                &on_progress,
            )?;
        }
        // Skip symlinks, special files, etc.
    }

    // Set the error log path in stats if any errors were logged
    if error_log.is_some() {
        stats.errors_log_path = Some(error_log_path.to_string_lossy().to_string());
    }

    Ok(stats)
}

struct GitArchiveEntry {
    archive_path: String,
    source_path: PathBuf,
    is_dir: bool,
}

struct CancellableReader<'a, R, C> {
    inner: R,
    should_cancel: &'a C,
}

impl<'a, R, C> CancellableReader<'a, R, C> {
    fn new(inner: R, should_cancel: &'a C) -> Self {
        Self {
            inner,
            should_cancel,
        }
    }
}

impl<R, C> Read for CancellableReader<'_, R, C>
where
    R: Read,
    C: Fn() -> bool,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if (self.should_cancel)() {
            return Err(io::Error::new(io::ErrorKind::Other, "scan cancelled"));
        }

        self.inner.read(buf)
    }
}

fn build_git_directory_archive<C>(git_dir: &Path, should_cancel: &C) -> Result<Vec<u8>>
where
    C: Fn() -> bool,
{
    let mut entries = Vec::new();

    for entry in WalkDir::new(git_dir).follow_links(false).min_depth(1) {
        if should_cancel() {
            bail!("scan cancelled");
        }

        let entry = entry.with_context(|| {
            format!(
                "failed to read .git archive entry under {}",
                git_dir.display()
            )
        })?;
        let file_type = entry.file_type();

        if !file_type.is_file() && !file_type.is_dir() {
            continue;
        }

        let relative = entry.path().strip_prefix(git_dir).with_context(|| {
            format!(
                "failed to compute .git archive path for {}",
                entry.path().display()
            )
        })?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        entries.push(GitArchiveEntry {
            archive_path: format!(".git/{relative}"),
            source_path: entry.path().to_path_buf(),
            is_dir: file_type.is_dir(),
        });
    }

    entries.sort_by(|left, right| left.archive_path.cmp(&right.archive_path));

    let mut archive = tar::Builder::new(Vec::new());
    for entry in entries {
        if should_cancel() {
            bail!("scan cancelled");
        }

        if entry.is_dir {
            let mut header = deterministic_tar_header(0, 0o755, tar::EntryType::Directory)?;
            archive
                .append_data(&mut header, Path::new(&entry.archive_path), io::empty())
                .with_context(|| format!("failed to append archive dir: {}", entry.archive_path))?;
        } else {
            let file = fs::File::open(&entry.source_path).with_context(|| {
                format!(
                    "failed to open .git archive file: {}",
                    entry.source_path.display()
                )
            })?;
            let file_size = file
                .metadata()
                .with_context(|| {
                    format!(
                        "failed to read .git archive file metadata: {}",
                        entry.source_path.display()
                    )
                })?
                .len();

            if should_cancel() {
                bail!("scan cancelled");
            }

            let mut header = deterministic_tar_header(file_size, 0o644, tar::EntryType::Regular)?;
            let reader = CancellableReader::new(file, should_cancel);
            archive
                .append_data(&mut header, Path::new(&entry.archive_path), reader)
                .with_context(|| {
                    format!("failed to append archive file: {}", entry.archive_path)
                })?;
        }
    }

    archive
        .into_inner()
        .context("failed to finish .git directory archive")
}

fn is_scan_cancelled_error(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|cause| cause.to_string() == "scan cancelled")
}

fn has_real_sibling_git_dir_for_git_tar_collision(path: &Path) -> bool {
    if !path
        .file_name()
        .map(|name| name == ".git.tar")
        .unwrap_or(false)
    {
        return false;
    }

    path.parent()
        .map(|parent| parent.join(".git"))
        .and_then(|git_dir| fs::symlink_metadata(git_dir).ok())
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

fn deterministic_tar_header(
    size: u64,
    mode: u32,
    entry_type: tar::EntryType,
) -> Result<tar::Header> {
    let mut header = tar::Header::new_gnu();
    header.set_size(size);
    header.set_mode(mode);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_username("")?;
    header.set_groupname("")?;
    header.set_entry_type(entry_type);
    Ok(header)
}

fn store_virtual_file<F>(
    virtual_path: &str,
    data: &[u8],
    modified: i64,
    created: i64,
    permissions: u32,
    content_store: &ContentStore,
    metadata_db: &MetadataDb,
    stats: &mut ScanStats,
    on_progress: &F,
) -> Result<()>
where
    F: Fn(&ScanProgress),
{
    let cid = cid_util::compute_cid(data);
    let cid_str = cid_util::cid_to_string(&cid);

    let was_new = !content_store.exists(&cid);
    let compressed_size = content_store
        .store(&cid, data)
        .with_context(|| format!("failed to store blob for: {virtual_path}"))?;

    if was_new {
        stats.unique_blobs += 1;
        stats.total_stored_bytes += compressed_size;
    } else {
        stats.duplicate_files += 1;
    }

    let file_meta = FileMetadata {
        cid: cid_util::cid_to_bytes(&cid),
        original_size: data.len() as u64,
        compressed_size,
        modified,
        created,
        permissions,
    };

    metadata_db
        .insert_file(virtual_path, &file_meta, &cid_str)
        .with_context(|| format!("failed to insert metadata for: {virtual_path}"))?;

    stats.total_files += 1;
    stats.total_original_bytes += data.len() as u64;

    on_progress(&ScanProgress {
        files_processed: stats.total_files,
        dirs_processed: stats.total_dirs,
        bytes_processed: stats.total_original_bytes,
        bytes_stored: stats.total_stored_bytes,
        duplicates_found: stats.duplicate_files,
        skipped_files: stats.skipped_files,
        current_file: virtual_path.to_string(),
    });

    Ok(())
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

fn extract_ctime(meta: &fs::Metadata) -> i64 {
    meta.created()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BuiltinScanPreset, ScanRule, ScanRuleAction};
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
    fn ignore_rule_skips_matching_directory_and_children() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir_all(source_dir.path().join("target/debug")).unwrap();
        fs::write(source_dir.path().join("target/debug/app"), b"binary").unwrap();
        fs::write(source_dir.path().join("src.rs"), b"source").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                rules: vec![ScanRule::new(r"(^|/)target$", ScanRuleAction::Ignore)],
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.skipped_files, 1);
        assert!(metadata_db.get_file("/repo/src.rs").unwrap().is_some());
        assert!(metadata_db
            .get_file("/repo/target/debug/app")
            .unwrap()
            .is_none());
        assert!(metadata_db.list_dir("/repo/target").unwrap().is_empty());
    }

    #[test]
    fn invalid_scan_rule_regex_fails_before_storing_metadata() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();
        fs::write(source_dir.path().join("file.txt"), b"content").unwrap();

        let err = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                rules: vec![ScanRule::new("[", ScanRuleAction::Ignore)],
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap_err();

        assert!(err.to_string().contains("invalid scan rule regex"));
        assert!(metadata_db.get_file("/repo/file.txt").unwrap().is_none());
    }

    #[test]
    fn default_scan_still_indexes_git_directory_entries() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir(source_dir.path().join(".git")).unwrap();
        fs::write(
            source_dir.path().join(".git/HEAD"),
            b"ref: refs/heads/main\n",
        )
        .unwrap();
        fs::create_dir_all(source_dir.path().join(".git/refs/heads")).unwrap();
        fs::write(source_dir.path().join(".git/refs/heads/main"), b"abc123\n").unwrap();

        let stats = scan_directory_into(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 2);
        assert!(metadata_db.get_file("/repo/.git/HEAD").unwrap().is_some());
        assert!(metadata_db
            .get_file("/repo/.git/refs/heads/main")
            .unwrap()
            .is_some());
        assert!(metadata_db.get_file("/repo/.git.tar").unwrap().is_none());
    }

    #[test]
    fn scan_options_default_does_not_bundle_git_directories() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir(source_dir.path().join(".git")).unwrap();
        fs::write(
            source_dir.path().join(".git/HEAD"),
            b"ref: refs/heads/main\n",
        )
        .unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions::default(),
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 1);
        assert!(metadata_db.get_file("/repo/.git/HEAD").unwrap().is_some());
        assert!(metadata_db.get_file("/repo/.git.tar").unwrap().is_none());
    }

    #[test]
    fn bundle_git_dirs_stores_single_git_tar_file() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir(source_dir.path().join(".git")).unwrap();
        fs::write(
            source_dir.path().join(".git/HEAD"),
            b"ref: refs/heads/main\n",
        )
        .unwrap();
        fs::create_dir_all(source_dir.path().join(".git/refs/heads")).unwrap();
        fs::write(source_dir.path().join(".git/refs/heads/main"), b"abc123\n").unwrap();
        fs::write(source_dir.path().join("tracked.txt"), b"tracked\n").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                bundle_git_dirs: true,
                rules: Vec::new(),
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 2);
        assert!(metadata_db.get_file("/repo/tracked.txt").unwrap().is_some());

        let git_tar_meta = metadata_db
            .get_file("/repo/.git.tar")
            .unwrap()
            .expect("expected .git.tar metadata");
        assert!(metadata_db.get_file("/repo/.git/HEAD").unwrap().is_none());
        assert!(metadata_db
            .get_file("/repo/.git/refs/heads/main")
            .unwrap()
            .is_none());

        let git_tar_cid = cid_util::cid_from_bytes(&git_tar_meta.cid).unwrap();
        let git_tar_data = content_store.read(&git_tar_cid).unwrap();
        let mut archive = tar::Archive::new(std::io::Cursor::new(git_tar_data));
        let mut archive_paths: Vec<String> = archive
            .entries()
            .unwrap()
            .map(|entry| {
                entry
                    .unwrap()
                    .path()
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        archive_paths.sort();

        assert!(archive_paths.contains(&".git/HEAD".to_string()));
        assert!(archive_paths.contains(&".git/refs/heads/main".to_string()));
    }

    #[test]
    fn bundled_identical_git_dirs_are_counted_as_duplicates() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        for repo in ["one", "two"] {
            let git_dir = source_dir.path().join(repo).join(".git");
            fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
            fs::write(git_dir.join("HEAD"), b"ref: refs/heads/main\n").unwrap();
            fs::write(git_dir.join("refs/heads/main"), b"abc123\n").unwrap();
        }

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                bundle_git_dirs: true,
                rules: Vec::new(),
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.unique_blobs, 1);
        assert_eq!(stats.duplicate_files, 1);

        let one_git_tar_meta = metadata_db
            .get_file("/repo/one/.git.tar")
            .unwrap()
            .expect("expected one/.git.tar metadata");
        let two_git_tar_meta = metadata_db
            .get_file("/repo/two/.git.tar")
            .unwrap()
            .expect("expected two/.git.tar metadata");
        assert_eq!(one_git_tar_meta.cid, two_git_tar_meta.cid);
        assert!(metadata_db
            .get_file("/repo/one/.git/HEAD")
            .unwrap()
            .is_none());
        assert!(metadata_db
            .get_file("/repo/two/.git/HEAD")
            .unwrap()
            .is_none());
    }

    #[test]
    fn bundle_git_dirs_skips_real_file_colliding_with_git_tar_path() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir(source_dir.path().join(".git")).unwrap();
        fs::write(
            source_dir.path().join(".git/HEAD"),
            b"ref: refs/heads/main\n",
        )
        .unwrap();
        fs::write(source_dir.path().join(".git.tar"), b"real file bytes").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                bundle_git_dirs: true,
                rules: Vec::new(),
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.skipped_files, 1);

        let git_tar_meta = metadata_db
            .get_file("/repo/.git.tar")
            .unwrap()
            .expect("expected .git.tar metadata");
        let git_tar_cid = cid_util::cid_from_bytes(&git_tar_meta.cid).unwrap();
        let git_tar_data = content_store.read(&git_tar_cid).unwrap();

        assert_ne!(git_tar_data, b"real file bytes");

        let mut archive = tar::Archive::new(std::io::Cursor::new(git_tar_data));
        let archive_paths: Vec<String> = archive
            .entries()
            .unwrap()
            .map(|entry| {
                entry
                    .unwrap()
                    .path()
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        assert!(archive_paths.contains(&".git/HEAD".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn bundle_git_dirs_does_not_bundle_symlink_named_git() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        let outside_dir = TempDir::new().unwrap();
        fs::write(outside_dir.path().join("HEAD"), b"outside git data").unwrap();
        std::os::unix::fs::symlink(outside_dir.path(), source_dir.path().join(".git")).unwrap();

        let _stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                bundle_git_dirs: true,
                rules: Vec::new(),
            },
            |_| {},
        )
        .unwrap();

        assert!(metadata_db.get_file("/repo/.git.tar").unwrap().is_none());
    }

    #[test]
    fn bundle_git_dirs_skips_real_directory_colliding_with_git_tar_path() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir(source_dir.path().join(".git")).unwrap();
        fs::write(
            source_dir.path().join(".git/HEAD"),
            b"ref: refs/heads/main\n",
        )
        .unwrap();
        fs::create_dir(source_dir.path().join(".git.tar")).unwrap();
        fs::write(source_dir.path().join(".git.tar/nested"), b"nested").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                bundle_git_dirs: true,
                rules: Vec::new(),
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.skipped_files, 1);
        assert!(metadata_db.get_file("/repo/.git.tar").unwrap().is_some());
        let repo_entries = metadata_db.list_dir("/repo").unwrap();
        let git_tar_entry = repo_entries
            .iter()
            .find(|entry| entry.name == ".git.tar")
            .expect("expected .git.tar entry");
        assert!(!git_tar_entry.is_dir);
        assert!(metadata_db
            .get_file("/repo/.git.tar/nested")
            .unwrap()
            .is_none());
    }

    #[test]
    fn build_git_archive_observes_cancellation_while_streaming_file() {
        let source_dir = TempDir::new().unwrap();
        let git_dir = source_dir.path().join(".git");

        fs::create_dir(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), b"ref: refs/heads/main\n").unwrap();

        use std::sync::atomic::{AtomicUsize, Ordering};
        let cancel_checks = AtomicUsize::new(0);
        let result = build_git_directory_archive(&git_dir, &|| {
            cancel_checks.fetch_add(1, Ordering::SeqCst) >= 3
        });

        let err = result.unwrap_err();
        assert!(is_scan_cancelled_error(&err));
    }

    #[test]
    fn bundled_git_scan_honors_cancellation() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        let git_dir = source_dir.path().join(".git");
        let objects_dir = git_dir.join("objects/aa");
        fs::create_dir_all(&objects_dir).unwrap();
        fs::write(git_dir.join("HEAD"), b"ref: refs/heads/main\n").unwrap();
        for index in 0..16 {
            fs::write(
                objects_dir.join(format!("{index:02x}")),
                format!("object {index}\n"),
            )
            .unwrap();
        }

        use std::sync::atomic::{AtomicUsize, Ordering};
        const CANCEL_AFTER_CHECKS: usize = 6;
        let cancel_checks = AtomicUsize::new(0);
        let result = scan_directory_into_with_options_and_cancellation(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                bundle_git_dirs: true,
                rules: Vec::new(),
            },
            |_| {},
            || cancel_checks.fetch_add(1, Ordering::SeqCst) >= CANCEL_AFTER_CHECKS,
        );

        let err = result.unwrap_err();
        assert!(is_scan_cancelled_error(&err));
        assert!(cancel_checks.load(Ordering::SeqCst) > CANCEL_AFTER_CHECKS);
        assert!(metadata_db.get_file("/repo/.git.tar").unwrap().is_none());
        assert!(!store_dir.path().join("errors_repo.log").exists());
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

    #[test]
    fn cancellable_scan_stops_after_cancellation_request() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("a.txt"), b"aaa").unwrap();
        fs::write(source_dir.path().join("b.txt"), b"bbb").unwrap();

        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        let cancelled = AtomicBool::new(false);
        let progress_count = AtomicU64::new(0);

        let result = scan_directory_into_with_cancellation(
            source_dir.path(),
            "/",
            store_dir.path(),
            &content_store,
            &metadata_db,
            |_p| {
                progress_count.fetch_add(1, Ordering::Relaxed);
                cancelled.store(true, Ordering::Relaxed);
            },
            || cancelled.load(Ordering::Relaxed),
        );

        assert!(result.unwrap_err().to_string().contains("scan cancelled"));
        assert_eq!(progress_count.load(Ordering::Relaxed), 1);

        let entries = metadata_db.list_dir("/").unwrap();
        let files: Vec<_> = entries.iter().filter(|entry| !entry.is_dir).collect();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn builtin_scan_presets_expand_to_rules() {
        let git = ScanRule::builtin(BuiltinScanPreset::Git);
        assert_eq!(git.pattern, r"(^|/)\.git$");
        assert_eq!(git.action, ScanRuleAction::Archive);

        let rust = ScanRule::builtin(BuiltinScanPreset::RustTarget);
        assert_eq!(rust.pattern, r"(^|/)target$");
        assert_eq!(rust.action, ScanRuleAction::Ignore);

        let node = ScanRule::builtin(BuiltinScanPreset::NodeModules);
        assert_eq!(node.pattern, r"(^|/)node_modules$");
        assert_eq!(node.action, ScanRuleAction::Ignore);

        let python = ScanRule::builtin(BuiltinScanPreset::PythonVenv);
        assert_eq!(python.pattern, r"(^|/)(\.venv|venv)$");
        assert_eq!(python.action, ScanRuleAction::Ignore);
    }

    #[test]
    fn scan_options_default_has_no_rules() {
        let options = ScanOptions::default();
        assert!(!options.bundle_git_dirs);
        assert!(options.rules.is_empty());
    }
}
