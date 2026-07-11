use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;
use std::time::UNIX_EPOCH;

use anyhow::{bail, Context, Result};
use rayon::ThreadPoolBuilder;
use regex::Regex;
use walkdir::WalkDir;

use crate::cid as cid_util;
use crate::content_store::ContentStore;
use crate::metadata::MetadataDb;
use crate::types::{
    BuiltinScanPreset, DirMetadata, FileMetadata, ScanOptions, ScanProgress, ScanRule,
    ScanRuleAction, ScanStats,
};

const METADATA_BATCH_THRESHOLD: usize = 1024;

#[derive(Default)]
struct MetadataBatch {
    files: Vec<(String, FileMetadata, String)>,
    dirs: Vec<(String, DirMetadata)>,
}

impl MetadataBatch {
    fn len(&self) -> usize {
        self.files.len() + self.dirs.len()
    }

    fn is_empty(&self) -> bool {
        self.files.is_empty() && self.dirs.is_empty()
    }

    fn push_file(&mut self, virtual_path: String, file_meta: FileMetadata, cid_str: String) {
        self.files.push((virtual_path, file_meta, cid_str));
    }

    fn push_dir(&mut self, virtual_path: String, dir_meta: DirMetadata) {
        self.dirs.push((virtual_path, dir_meta));
    }

    fn flush_if_full(&mut self, metadata_db: &MetadataDb) -> Result<()> {
        if self.len() >= METADATA_BATCH_THRESHOLD {
            self.flush(metadata_db, false)?;
        }
        Ok(())
    }

    fn flush(&mut self, metadata_db: &MetadataDb, durable: bool) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }

        metadata_db.write_batch(&self.files, &self.dirs, durable)?;
        self.files.clear();
        self.dirs.clear();
        Ok(())
    }
}

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

enum WriteOp {
    File {
        virtual_path: String,
        meta: FileMetadata,
        cid_str: String,
        preexisting: bool,
        original_size: u64,
        compressed_size: u64,
    },
    Dir {
        virtual_path: String,
        meta: DirMetadata,
    },
    Unchanged {
        virtual_path: String,
        original_size: u64,
    },
    Skipped {
        virtual_path: String,
    },
    Error {
        path: String,
        message: String,
        still_exists: bool,
    },
}

struct ScanContext<'a> {
    source: &'a Path,
    target_prefix: &'a str,
    store_root: &'a Path,
    content_store: &'a ContentStore,
    metadata_db: &'a MetadataDb,
}

fn effective_workers(opt: &ScanOptions) -> usize {
    match opt.parallelism {
        Some(workers) => workers.max(1),
        None => thread::available_parallelism()
            .map(|workers| workers.get())
            .unwrap_or(1)
            .min(8),
    }
}

fn is_tiny_tree(source: &Path) -> bool {
    WalkDir::new(source).into_iter().take(64).count() < 64
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
        unchanged_files: stats.unchanged_files,
        current_file: virtual_path,
    });
}

#[allow(clippy::too_many_arguments)]
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

    let workers = effective_workers(&options);
    let tiny_tree = is_tiny_tree(&source);
    let context = ScanContext {
        source: &source,
        target_prefix,
        store_root,
        content_store,
        metadata_db,
    };

    if workers <= 1 || (options.parallelism.is_none() && tiny_tree) {
        return scan_serial_directory_into_with_options_and_cancellation(
            &context,
            options,
            on_progress,
            should_cancel,
        );
    }

    scan_parallel(&context, options, workers, on_progress, should_cancel)
}

fn scan_serial_directory_into_with_options_and_cancellation<F, C>(
    context: &ScanContext<'_>,
    options: ScanOptions,
    on_progress: F,
    should_cancel: C,
) -> Result<ScanStats>
where
    F: Fn(&ScanProgress),
    C: Fn() -> bool,
{
    let source = context.source;
    let target_prefix = context.target_prefix;
    let store_root = context.store_root;
    let content_store = context.content_store;
    let metadata_db = context.metadata_db;

    let compiled_rules = compile_scan_rules(&options)?;
    let prune = options.prune_deleted;

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
    let mut seen: HashSet<String> = HashSet::new();
    let mut metadata_batch = MetadataBatch::default();

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

    let mut walker = WalkDir::new(source).follow_links(false).into_iter();
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
        let relative = match abs_path.strip_prefix(source) {
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

        let matched_rule = if rel_str.is_empty() {
            None
        } else {
            matching_rule(&compiled_rules, &rel_str)
        };

        if let Some(rule) = matched_rule {
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

                    let archive_virtual_path = format!("{virtual_path}.tar");
                    if prune {
                        seen.insert(archive_virtual_path.clone());
                    }
                    let root_name = archive_root_name(abs_path);
                    let archive_data =
                        match build_directory_archive(&root_name, abs_path, &should_cancel) {
                            Ok(data) => data,
                            Err(err) => {
                                if is_scan_cancelled_error(&err) {
                                    return Err(err);
                                }

                                let msg = format!("failed to archive directory: {err}");
                                (|| {
                                    log_error!(
                                        &mut error_log,
                                        error_log_path,
                                        msg,
                                        archive_virtual_path.clone()
                                    )
                                })();
                                stats.skipped_files += 1;
                                emit_skipped_progress(archive_virtual_path, &stats, &on_progress);
                                walker.skip_current_dir();
                                continue;
                            }
                        };

                    let fs_meta = match fs::metadata(abs_path) {
                        Ok(m) => m,
                        Err(err) => {
                            let msg = format!("failed to read metadata: {err}");
                            let display = abs_path.display().to_string();
                            (|| log_error!(&mut error_log, error_log_path, msg, display))();
                            stats.skipped_files += 1;
                            emit_skipped_progress(archive_virtual_path, &stats, &on_progress);
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
                        &mut metadata_batch,
                        &mut stats,
                        &on_progress,
                    )?;

                    walker.skip_current_dir();
                    continue;
                }
            }
        }

        if has_sibling_archive_source_for_rule(source, &rel_str, &compiled_rules) {
            let msg = "virtual path collision: .tar is reserved for archived directory".to_string();
            (|| log_error!(&mut error_log, error_log_path, msg, virtual_path.clone()))();
            if prune {
                seen.insert(virtual_path.clone());
            }
            stats.skipped_files += 1;
            emit_skipped_progress(virtual_path, &stats, &on_progress);
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
                if prune {
                    seen.insert(virtual_path.clone());
                }
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
            metadata_batch.push_dir(virtual_path.clone(), dir_meta);
            metadata_batch
                .flush_if_full(metadata_db)
                .with_context(|| format!("failed to insert dir: {virtual_path}"))?;

            stats.total_dirs += 1;
            if prune {
                seen.insert(virtual_path.clone());
            }
        } else if fs_meta.is_file() {
            let modified = extract_mtime(&fs_meta);

            if let Some(existing) = unchanged_file_metadata(
                metadata_db,
                content_store,
                &virtual_path,
                fs_meta.len(),
                modified,
            ) {
                record_unchanged_file(&existing, &virtual_path, &mut stats, &on_progress);
                if prune {
                    seen.insert(virtual_path.clone());
                }
                continue;
            }

            // Read file content — skip on error
            let data = match fs::read(abs_path) {
                Ok(d) => d,
                Err(err) => {
                    let msg = format!("failed to read file: {err}");
                    let display = abs_path.display().to_string();
                    (|| log_error!(&mut error_log, error_log_path, msg, display))();
                    if prune {
                        seen.insert(virtual_path.clone());
                    }
                    stats.skipped_files += 1;
                    on_progress(&ScanProgress {
                        files_processed: stats.total_files,
                        dirs_processed: stats.total_dirs,
                        bytes_processed: stats.total_original_bytes,
                        bytes_stored: stats.total_stored_bytes,
                        duplicates_found: stats.duplicate_files,
                        skipped_files: stats.skipped_files,
                        unchanged_files: stats.unchanged_files,
                        current_file: virtual_path,
                    });
                    continue;
                }
            };

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
                &mut metadata_batch,
                &mut stats,
                &on_progress,
            )?;
            if prune {
                seen.insert(virtual_path.clone());
            }
        }
        // Skip symlinks, special files, etc.
    }

    metadata_batch
        .flush(metadata_db, true)
        .context("failed to flush metadata batch")?;

    // Set the error log path in stats if any errors were logged
    if error_log.is_some() {
        stats.errors_log_path = Some(error_log_path.to_string_lossy().to_string());
    }

    if prune {
        let scope_prefix = if prefix.is_empty() {
            "/".to_string()
        } else {
            format!("{prefix}/")
        };
        stats.pruned_entries = metadata_db
            .prune_missing(&scope_prefix, &seen)
            .context("failed to prune deleted entries")?;
    }

    Ok(stats)
}

struct ParallelDispatchContext<'a> {
    source: &'a Path,
    content_store: &'a ContentStore,
    metadata_db: &'a MetadataDb,
    prefix: String,
    rules: Vec<CompiledScanRule>,
    workers: usize,
    cancel_flag: Arc<AtomicBool>,
    sender: mpsc::Sender<WriteOp>,
}

struct ParallelWriterShared<'a, F, C> {
    metadata_db: &'a MetadataDb,
    error_log_path: &'a Path,
    prune: bool,
    prefix: &'a str,
    cancel_flag: &'a AtomicBool,
    on_progress: &'a F,
    should_cancel: &'a C,
}

struct ParallelWriterState {
    stats: ScanStats,
    seen: HashSet<String>,
    cid_strings: HashSet<String>,
    metadata_batch: MetadataBatch,
    error_log: Option<fs::File>,
}

struct FileTaskContext<'a> {
    virtual_path: String,
    abs_path: PathBuf,
    modified: i64,
    created: i64,
    permissions: u32,
    content_store: &'a ContentStore,
    sender: mpsc::Sender<WriteOp>,
    cancel_flag: Arc<AtomicBool>,
}

struct ArchiveTaskContext<'a> {
    archive_virtual_path: String,
    root_name: String,
    abs_path: PathBuf,
    content_store: &'a ContentStore,
    sender: mpsc::Sender<WriteOp>,
    cancel_flag: Arc<AtomicBool>,
}

fn scan_parallel<F, C>(
    context: &ScanContext<'_>,
    options: ScanOptions,
    workers: usize,
    on_progress: F,
    should_cancel: C,
) -> Result<ScanStats>
where
    F: Fn(&ScanProgress),
    C: Fn() -> bool,
{
    let compiled_rules = compile_scan_rules(&options)?;
    let prune = options.prune_deleted;

    let prefix = if context.target_prefix == "/" || context.target_prefix.is_empty() {
        String::new()
    } else {
        let p = context.target_prefix.trim_end_matches('/');
        if p.starts_with('/') {
            p.to_string()
        } else {
            format!("/{p}")
        }
    };

    if !prefix.is_empty() {
        ensure_parent_dirs(context.metadata_db, &prefix)?;
    }

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
    let error_log_path = context.store_root.join(&error_log_name);

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel();

    thread::scope(|scope| {
        let dispatcher = scope.spawn({
            let dispatch_context = ParallelDispatchContext {
                source: context.source,
                content_store: context.content_store,
                metadata_db: context.metadata_db,
                prefix: prefix.clone(),
                rules: compiled_rules,
                workers,
                cancel_flag: Arc::clone(&cancel_flag),
                sender,
            };

            move || dispatch_parallel_scan(dispatch_context)
        });

        let writer_result = consume_parallel_write_ops(
            receiver,
            ParallelWriterShared {
                metadata_db: context.metadata_db,
                error_log_path: &error_log_path,
                prune,
                prefix: &prefix,
                cancel_flag: &cancel_flag,
                on_progress: &on_progress,
                should_cancel: &should_cancel,
            },
        );
        if writer_result.is_err() {
            cancel_flag.store(true, Ordering::Relaxed);
        }

        let dispatcher_result = dispatcher
            .join()
            .map_err(|_| anyhow::anyhow!("parallel scan dispatcher panicked"))?;
        dispatcher_result?;
        writer_result
    })
}

fn dispatch_parallel_scan(context: ParallelDispatchContext<'_>) -> Result<()> {
    let ParallelDispatchContext {
        source,
        content_store,
        metadata_db,
        prefix,
        rules,
        workers,
        cancel_flag,
        sender,
    } = context;

    let rules: Arc<[CompiledScanRule]> = rules.into();
    let walk_rules = Arc::clone(&rules);
    let walk_cancel_flag = Arc::clone(&cancel_flag);
    let walk_source = source.to_path_buf();
    let walk = jwalk::WalkDirGeneric::<((), ())>::new(source)
        .skip_hidden(false)
        .follow_links(false)
        .process_read_dir(move |_depth, _path, _state, children| {
            for child in children.iter_mut().filter_map(|child| child.as_mut().ok()) {
                if !child.file_type.is_dir() {
                    continue;
                }

                if walk_cancel_flag.load(Ordering::Relaxed) {
                    child.read_children_path = None;
                    continue;
                }

                let child_path = child.path();
                let Ok(relative) = child_path.strip_prefix(&walk_source) else {
                    continue;
                };
                let rel_str = relative.to_string_lossy().replace('\\', "/");
                if rel_str.is_empty() {
                    continue;
                }

                let matched_rule = matching_rule(walk_rules.as_ref(), &rel_str);
                let should_skip_children = matched_rule
                    .map(|rule| {
                        rule.action == ScanRuleAction::Ignore
                            || rule.action == ScanRuleAction::Archive
                    })
                    .unwrap_or(false)
                    || has_sibling_archive_source_for_rule(
                        &walk_source,
                        &rel_str,
                        walk_rules.as_ref(),
                    );

                if should_skip_children {
                    child.read_children_path = None;
                }
            }
        })
        .try_into_iter()
        .context("failed to start parallel directory walk")?;

    let pool = ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .context("failed to build scan thread pool")?;

    pool.scope(|scope| {
        for entry in walk {
            if cancel_flag.load(Ordering::Relaxed) {
                break;
            }

            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    let entry_path = err
                        .path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "<unknown>".to_string());
                    if !send_write_op(
                        &sender,
                        &cancel_flag,
                        WriteOp::Error {
                            path: entry_path,
                            message: err.to_string(),
                            still_exists: false,
                        },
                    ) {
                        break;
                    }
                    continue;
                }
            };

            let abs_path = entry.path();
            let entry_file_type = entry.file_type();
            let relative = match abs_path.strip_prefix(source) {
                Ok(relative) => relative,
                Err(err) => {
                    let display = abs_path.display().to_string();
                    if !send_write_op(
                        &sender,
                        &cancel_flag,
                        WriteOp::Error {
                            path: display,
                            message: err.to_string(),
                            still_exists: false,
                        },
                    ) {
                        break;
                    }
                    continue;
                }
            };

            let rel_str = relative.to_string_lossy().replace('\\', "/");
            let Some(virtual_path) = virtual_path_for(&prefix, &rel_str) else {
                continue;
            };

            // Plan each entry on the worker pool so stat, rule matching, and
            // change detection run concurrently instead of serially here.
            let task_rules = Arc::clone(&rules);
            let task_sender = sender.clone();
            let task_cancel_flag = Arc::clone(&cancel_flag);
            scope.spawn(move |_| {
                plan_walk_entry(PlanTaskContext {
                    source,
                    content_store,
                    metadata_db,
                    rules: task_rules,
                    rel_str,
                    virtual_path,
                    abs_path,
                    entry_is_dir: entry_file_type.is_dir(),
                    sender: task_sender,
                    cancel_flag: task_cancel_flag,
                });
            });
        }
    });

    Ok(())
}

struct PlanTaskContext<'a> {
    source: &'a Path,
    content_store: &'a ContentStore,
    metadata_db: &'a MetadataDb,
    rules: Arc<[CompiledScanRule]>,
    rel_str: String,
    virtual_path: String,
    abs_path: PathBuf,
    entry_is_dir: bool,
    sender: mpsc::Sender<WriteOp>,
    cancel_flag: Arc<AtomicBool>,
}

/// Per-entry planning stage, run on a worker thread: rule matching, stat,
/// change detection, and (for changed files / archives) the store work itself.
fn plan_walk_entry(context: PlanTaskContext<'_>) {
    let PlanTaskContext {
        source,
        content_store,
        metadata_db,
        rules,
        rel_str,
        virtual_path,
        abs_path,
        entry_is_dir,
        sender,
        cancel_flag,
    } = context;

    if cancel_flag.load(Ordering::Relaxed) {
        return;
    }

    let matched_rule = if rel_str.is_empty() {
        None
    } else {
        matching_rule(rules.as_ref(), &rel_str)
    };

    if let Some(rule) = matched_rule {
        match rule.action {
            ScanRuleAction::Ignore => {
                send_write_op(&sender, &cancel_flag, WriteOp::Skipped { virtual_path });
                return;
            }
            ScanRuleAction::Archive => {
                if !entry_is_dir {
                    send_write_op(&sender, &cancel_flag, WriteOp::Skipped { virtual_path });
                    return;
                }

                let archive_virtual_path = format!("{virtual_path}.tar");
                let root_name = archive_root_name(&abs_path);
                store_archive_write_op(ArchiveTaskContext {
                    archive_virtual_path,
                    root_name,
                    abs_path,
                    content_store,
                    sender,
                    cancel_flag,
                });
                return;
            }
        }
    }

    if has_sibling_archive_source_for_rule(source, &rel_str, rules.as_ref()) {
        let message = "virtual path collision: .tar is reserved for archived directory".to_string();
        send_write_op(
            &sender,
            &cancel_flag,
            WriteOp::Error {
                path: virtual_path,
                message,
                still_exists: true,
            },
        );
        return;
    }

    let fs_meta = match fs::metadata(&abs_path) {
        Ok(metadata) => metadata,
        Err(err) => {
            let message = format!("failed to read metadata: {err}");
            send_write_op(
                &sender,
                &cancel_flag,
                WriteOp::Error {
                    path: virtual_path,
                    message,
                    still_exists: true,
                },
            );
            return;
        }
    };

    if fs_meta.is_dir() {
        let modified = extract_mtime(&fs_meta);
        let dir_meta = DirMetadata {
            child_count: 0,
            modified,
        };
        send_write_op(
            &sender,
            &cancel_flag,
            WriteOp::Dir {
                virtual_path,
                meta: dir_meta,
            },
        );
    } else if fs_meta.is_file() {
        let modified = extract_mtime(&fs_meta);
        if let Some(existing) = unchanged_file_metadata(
            metadata_db,
            content_store,
            &virtual_path,
            fs_meta.len(),
            modified,
        ) {
            send_write_op(
                &sender,
                &cancel_flag,
                WriteOp::Unchanged {
                    virtual_path,
                    original_size: existing.original_size,
                },
            );
            return;
        }

        let created = extract_ctime(&fs_meta);
        let permissions = metadata_permissions(&fs_meta);
        store_file_write_op(FileTaskContext {
            virtual_path,
            abs_path,
            modified,
            created,
            permissions,
            content_store,
            sender,
            cancel_flag,
        });
    }
}

fn consume_parallel_write_ops<F, C>(
    receiver: mpsc::Receiver<WriteOp>,
    shared: ParallelWriterShared<'_, F, C>,
) -> Result<ScanStats>
where
    F: Fn(&ScanProgress),
    C: Fn() -> bool,
{
    let mut state = ParallelWriterState {
        stats: ScanStats::new(),
        seen: HashSet::new(),
        cid_strings: HashSet::new(),
        metadata_batch: MetadataBatch::default(),
        error_log: None,
    };

    loop {
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(op) => {
                handle_write_op(op, &mut state, &shared)?;
                if (shared.should_cancel)() {
                    shared.cancel_flag.store(true, Ordering::Relaxed);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if (shared.should_cancel)() {
                    shared.cancel_flag.store(true, Ordering::Relaxed);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    if shared.cancel_flag.load(Ordering::Relaxed) {
        bail!("scan cancelled");
    }

    state
        .metadata_batch
        .flush(shared.metadata_db, true)
        .context("failed to flush metadata batch")?;

    if state.error_log.is_some() {
        state.stats.errors_log_path = Some(shared.error_log_path.to_string_lossy().to_string());
    }

    if shared.prune {
        let scope_prefix = if shared.prefix.is_empty() {
            "/".to_string()
        } else {
            format!("{}/", shared.prefix)
        };
        state.stats.pruned_entries = shared
            .metadata_db
            .prune_missing(&scope_prefix, &state.seen)
            .context("failed to prune deleted entries")?;
    }

    Ok(state.stats)
}

fn handle_write_op<F, C>(
    op: WriteOp,
    state: &mut ParallelWriterState,
    shared: &ParallelWriterShared<'_, F, C>,
) -> Result<()>
where
    F: Fn(&ScanProgress),
    C: Fn() -> bool,
{
    match op {
        WriteOp::File {
            virtual_path,
            meta,
            cid_str,
            preexisting,
            original_size,
            compressed_size,
        } => {
            if preexisting {
                state.stats.duplicate_files += 1;
            } else if state.cid_strings.insert(cid_str.clone()) {
                state.stats.unique_blobs += 1;
                state.stats.total_stored_bytes += compressed_size;
            } else {
                state.stats.duplicate_files += 1;
            }

            state
                .metadata_batch
                .push_file(virtual_path.clone(), meta, cid_str);
            state
                .metadata_batch
                .flush_if_full(shared.metadata_db)
                .with_context(|| format!("failed to insert metadata for: {virtual_path}"))?;

            state.stats.total_files += 1;
            state.stats.total_original_bytes += original_size;
            if shared.prune {
                state.seen.insert(virtual_path.clone());
            }

            (shared.on_progress)(&ScanProgress {
                files_processed: state.stats.total_files,
                dirs_processed: state.stats.total_dirs,
                bytes_processed: state.stats.total_original_bytes,
                bytes_stored: state.stats.total_stored_bytes,
                duplicates_found: state.stats.duplicate_files,
                skipped_files: state.stats.skipped_files,
                unchanged_files: state.stats.unchanged_files,
                current_file: virtual_path,
            });
        }
        WriteOp::Dir { virtual_path, meta } => {
            state.metadata_batch.push_dir(virtual_path.clone(), meta);
            state
                .metadata_batch
                .flush_if_full(shared.metadata_db)
                .with_context(|| format!("failed to insert dir: {virtual_path}"))?;

            state.stats.total_dirs += 1;
            if shared.prune {
                state.seen.insert(virtual_path);
            }
        }
        WriteOp::Unchanged {
            virtual_path,
            original_size,
        } => {
            state.stats.total_files += 1;
            state.stats.total_original_bytes += original_size;
            state.stats.unchanged_files += 1;
            if shared.prune {
                state.seen.insert(virtual_path.clone());
            }

            (shared.on_progress)(&ScanProgress {
                files_processed: state.stats.total_files,
                dirs_processed: state.stats.total_dirs,
                bytes_processed: state.stats.total_original_bytes,
                bytes_stored: state.stats.total_stored_bytes,
                duplicates_found: state.stats.duplicate_files,
                skipped_files: state.stats.skipped_files,
                unchanged_files: state.stats.unchanged_files,
                current_file: virtual_path,
            });
        }
        WriteOp::Skipped { virtual_path } => {
            state.stats.skipped_files += 1;
            emit_skipped_progress(virtual_path, &state.stats, shared.on_progress);
        }
        WriteOp::Error {
            path,
            message,
            still_exists,
        } => {
            log_parallel_error(&mut state.error_log, shared.error_log_path, &path, &message);
            state.stats.skipped_files += 1;
            if still_exists && shared.prune {
                state.seen.insert(path);
            }
        }
    }

    Ok(())
}

fn send_write_op(sender: &mpsc::Sender<WriteOp>, cancel_flag: &AtomicBool, op: WriteOp) -> bool {
    if sender.send(op).is_ok() {
        true
    } else {
        cancel_flag.store(true, Ordering::Relaxed);
        false
    }
}

fn store_file_write_op(context: FileTaskContext<'_>) {
    let FileTaskContext {
        virtual_path,
        abs_path,
        modified,
        created,
        permissions,
        content_store,
        sender,
        cancel_flag,
    } = context;

    if cancel_flag.load(Ordering::Relaxed) {
        return;
    }

    let data = match fs::read(&abs_path) {
        Ok(data) => data,
        Err(err) => {
            let message = format!("failed to read file: {err}");
            send_write_op(
                &sender,
                &cancel_flag,
                WriteOp::Error {
                    path: virtual_path,
                    message,
                    still_exists: true,
                },
            );
            return;
        }
    };

    match store_parallel_file_data(
        &virtual_path,
        &data,
        modified,
        created,
        permissions,
        content_store,
    ) {
        Ok((meta, cid_str, preexisting, original_size, compressed_size)) => {
            send_write_op(
                &sender,
                &cancel_flag,
                WriteOp::File {
                    virtual_path,
                    meta,
                    cid_str,
                    preexisting,
                    original_size,
                    compressed_size,
                },
            );
        }
        Err(err) => {
            send_write_op(
                &sender,
                &cancel_flag,
                WriteOp::Error {
                    path: virtual_path,
                    message: err.to_string(),
                    still_exists: true,
                },
            );
        }
    }
}

fn store_archive_write_op(context: ArchiveTaskContext<'_>) {
    let ArchiveTaskContext {
        archive_virtual_path,
        root_name,
        abs_path,
        content_store,
        sender,
        cancel_flag,
    } = context;

    if cancel_flag.load(Ordering::Relaxed) {
        return;
    }

    let archive_cancel_flag = Arc::clone(&cancel_flag);
    let should_cancel = || archive_cancel_flag.load(Ordering::Relaxed);
    let archive_data = match build_directory_archive(&root_name, &abs_path, &should_cancel) {
        Ok(data) => data,
        Err(err) => {
            if is_scan_cancelled_error(&err) {
                cancel_flag.store(true, Ordering::Relaxed);
                return;
            }

            let message = format!("failed to archive directory: {err}");
            send_write_op(
                &sender,
                &cancel_flag,
                WriteOp::Error {
                    path: archive_virtual_path,
                    message,
                    still_exists: true,
                },
            );
            return;
        }
    };

    let fs_meta = match fs::metadata(&abs_path) {
        Ok(metadata) => metadata,
        Err(err) => {
            let message = format!("failed to read metadata: {err}");
            send_write_op(
                &sender,
                &cancel_flag,
                WriteOp::Error {
                    path: archive_virtual_path,
                    message,
                    still_exists: true,
                },
            );
            return;
        }
    };

    let modified = extract_mtime(&fs_meta);
    let created = extract_ctime(&fs_meta);
    match store_parallel_file_data(
        &archive_virtual_path,
        &archive_data,
        modified,
        created,
        0o644,
        content_store,
    ) {
        Ok((meta, cid_str, preexisting, original_size, compressed_size)) => {
            send_write_op(
                &sender,
                &cancel_flag,
                WriteOp::File {
                    virtual_path: archive_virtual_path,
                    meta,
                    cid_str,
                    preexisting,
                    original_size,
                    compressed_size,
                },
            );
        }
        Err(err) => {
            send_write_op(
                &sender,
                &cancel_flag,
                WriteOp::Error {
                    path: archive_virtual_path,
                    message: err.to_string(),
                    still_exists: true,
                },
            );
        }
    }
}

fn store_parallel_file_data(
    virtual_path: &str,
    data: &[u8],
    modified: i64,
    created: i64,
    permissions: u32,
    content_store: &ContentStore,
) -> Result<(FileMetadata, String, bool, u64, u64)> {
    let cid = cid_util::compute_cid(data);
    let cid_str = cid_util::cid_to_string(&cid);
    let preexisting = content_store.exists(&cid);
    let compressed_size = content_store
        .store(&cid, data)
        .with_context(|| format!("failed to store blob for: {virtual_path}"))?;
    let original_size = data.len() as u64;
    let meta = FileMetadata {
        cid: cid_util::cid_to_bytes(&cid),
        original_size,
        compressed_size,
        modified,
        created,
        permissions,
    };

    Ok((meta, cid_str, preexisting, original_size, compressed_size))
}

fn virtual_path_for(prefix: &str, rel_str: &str) -> Option<String> {
    if rel_str.is_empty() {
        if prefix.is_empty() {
            None
        } else {
            Some(prefix.to_string())
        }
    } else {
        Some(format!("{prefix}/{rel_str}"))
    }
}

fn metadata_permissions(meta: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode()
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0o644u32
    }
}

fn log_parallel_error(
    error_log: &mut Option<fs::File>,
    error_log_path: &Path,
    entry_path: &str,
    err: &str,
) {
    let file = match error_log {
        Some(file) => file,
        None => {
            let opened = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(error_log_path)
                .ok();
            match opened {
                Some(file) => {
                    *error_log = Some(file);
                    error_log.as_mut().expect("error log was just opened")
                }
                None => return,
            }
        }
    };
    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let _ = writeln!(file, "[{timestamp}] {entry_path}: {err}");
}

struct DirectoryArchiveEntry {
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
            return Err(io::Error::other("scan cancelled"));
        }

        self.inner.read(buf)
    }
}

fn build_directory_archive<C>(
    archive_root_name: &str,
    dir: &Path,
    should_cancel: &C,
) -> Result<Vec<u8>>
where
    C: Fn() -> bool,
{
    let mut entries = Vec::new();

    for entry in WalkDir::new(dir).follow_links(false).min_depth(1) {
        if should_cancel() {
            bail!("scan cancelled");
        }

        let entry = entry.with_context(|| {
            format!(
                "failed to read directory archive entry under {}",
                dir.display()
            )
        })?;
        let file_type = entry.file_type();

        if !file_type.is_file() && !file_type.is_dir() {
            continue;
        }

        let relative = entry.path().strip_prefix(dir).with_context(|| {
            format!(
                "failed to compute directory archive path for {}",
                entry.path().display()
            )
        })?;
        entries.push(DirectoryArchiveEntry {
            archive_path: archive_relative_path(archive_root_name, relative),
            source_path: entry.path().to_path_buf(),
            is_dir: file_type.is_dir(),
        });
    }

    entries.sort_by(|left, right| left.archive_path.cmp(&right.archive_path));

    let mut archive = tar::Builder::new(Vec::new());
    if entries.is_empty() {
        if should_cancel() {
            bail!("scan cancelled");
        }

        let mut header = deterministic_tar_header(0, 0o755, tar::EntryType::Directory)?;
        archive
            .append_data(&mut header, Path::new(archive_root_name), io::empty())
            .with_context(|| format!("failed to append archive dir: {archive_root_name}"))?;
    }

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
                    "failed to open directory archive file: {}",
                    entry.source_path.display()
                )
            })?;
            let file_size = file
                .metadata()
                .with_context(|| {
                    format!(
                        "failed to read directory archive file metadata: {}",
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
        .context("failed to finish directory archive")
}

fn archive_relative_path(archive_root_name: &str, relative: &Path) -> String {
    let mut parts = vec![archive_root_name.to_string()];
    for component in relative.components() {
        parts.push(component.as_os_str().to_string_lossy().to_string());
    }
    parts.join("/")
}

fn archive_root_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "archive".to_string())
}

fn is_scan_cancelled_error(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|cause| cause.to_string() == "scan cancelled")
}

fn archive_collision_source_relative(relative_path: &str) -> Option<String> {
    relative_path
        .strip_suffix(".tar")
        .filter(|source_relative| !source_relative.is_empty() && !source_relative.ends_with('/'))
        .map(|source_relative| source_relative.to_string())
}

fn has_sibling_archive_source_for_rule(
    source_root: &Path,
    relative_path: &str,
    rules: &[CompiledScanRule],
) -> bool {
    let Some(source_relative) = archive_collision_source_relative(relative_path) else {
        return false;
    };

    let sibling = source_root.join(&source_relative);
    let Ok(metadata) = fs::symlink_metadata(&sibling) else {
        return false;
    };
    if !metadata.is_dir() {
        return false;
    }

    matching_rule(rules, &source_relative)
        .map(|rule| rule.action == ScanRuleAction::Archive)
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

#[allow(clippy::too_many_arguments)]
fn store_virtual_file<F>(
    virtual_path: &str,
    data: &[u8],
    modified: i64,
    created: i64,
    permissions: u32,
    content_store: &ContentStore,
    metadata_db: &MetadataDb,
    metadata_batch: &mut MetadataBatch,
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

    metadata_batch.push_file(virtual_path.to_string(), file_meta, cid_str);
    metadata_batch
        .flush_if_full(metadata_db)
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
        unchanged_files: stats.unchanged_files,
        current_file: virtual_path.to_string(),
    });

    Ok(())
}

fn unchanged_file_metadata(
    metadata_db: &MetadataDb,
    content_store: &ContentStore,
    virtual_path: &str,
    fs_size: u64,
    modified: i64,
) -> Option<FileMetadata> {
    let existing = metadata_db.get_file(virtual_path).ok().flatten()?;
    if existing.original_size != fs_size || existing.modified != modified {
        return None;
    }
    let cid = cid_util::cid_from_bytes(&existing.cid).ok()?;
    if !content_store.exists(&cid) {
        return None;
    }
    Some(existing)
}

fn record_unchanged_file<F>(
    existing: &FileMetadata,
    virtual_path: &str,
    stats: &mut ScanStats,
    on_progress: &F,
) where
    F: Fn(&ScanProgress),
{
    stats.total_files += 1;
    stats.total_original_bytes += existing.original_size;
    stats.unchanged_files += 1;

    on_progress(&ScanProgress {
        files_processed: stats.total_files,
        dirs_processed: stats.total_dirs,
        bytes_processed: stats.total_original_bytes,
        bytes_stored: stats.total_stored_bytes,
        duplicates_found: stats.duplicate_files,
        skipped_files: stats.skipped_files,
        unchanged_files: stats.unchanged_files,
        current_file: virtual_path.to_string(),
    });
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
    fn builtin_ignore_presets_skip_common_project_artifacts() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir_all(source_dir.path().join("target/debug")).unwrap();
        fs::write(source_dir.path().join("target/debug/app"), b"rust").unwrap();
        fs::create_dir_all(source_dir.path().join("app/node_modules/react")).unwrap();
        fs::write(
            source_dir.path().join("app/node_modules/react/index.js"),
            b"node",
        )
        .unwrap();
        fs::create_dir_all(source_dir.path().join("service/.venv/bin")).unwrap();
        fs::write(
            source_dir.path().join("service/.venv/bin/python"),
            b"python",
        )
        .unwrap();
        fs::create_dir_all(source_dir.path().join("other/venv/bin")).unwrap();
        fs::write(source_dir.path().join("other/venv/bin/python"), b"python").unwrap();
        fs::write(source_dir.path().join("keep.txt"), b"keep").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                rules: vec![
                    ScanRule::builtin(BuiltinScanPreset::RustTarget),
                    ScanRule::builtin(BuiltinScanPreset::NodeModules),
                    ScanRule::builtin(BuiltinScanPreset::PythonVenv),
                ],
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.skipped_files, 4);
        assert!(metadata_db.get_file("/repo/keep.txt").unwrap().is_some());
        assert!(metadata_db
            .get_file("/repo/target/debug/app")
            .unwrap()
            .is_none());
        assert!(metadata_db
            .get_file("/repo/app/node_modules/react/index.js")
            .unwrap()
            .is_none());
        assert!(metadata_db
            .get_file("/repo/service/.venv/bin/python")
            .unwrap()
            .is_none());
        assert!(metadata_db
            .get_file("/repo/other/venv/bin/python")
            .unwrap()
            .is_none());
    }

    #[test]
    fn archive_rule_stores_matching_directory_as_tar() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir_all(source_dir.path().join("cache/sub")).unwrap();
        fs::write(source_dir.path().join("cache/sub/item"), b"cached").unwrap();
        fs::write(source_dir.path().join("keep.txt"), b"keep").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                rules: vec![ScanRule::new(r"(^|/)cache$", ScanRuleAction::Archive)],
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 2);
        assert!(metadata_db.get_file("/repo/cache.tar").unwrap().is_some());
        assert!(metadata_db
            .get_file("/repo/cache/sub/item")
            .unwrap()
            .is_none());
        assert!(metadata_db.get_file("/repo/keep.txt").unwrap().is_some());

        let archive_meta = metadata_db.get_file("/repo/cache.tar").unwrap().unwrap();
        let archive_cid = cid_util::cid_from_bytes(&archive_meta.cid).unwrap();
        let archive_bytes = content_store.read(&archive_cid).unwrap();
        let mut archive = tar::Archive::new(std::io::Cursor::new(archive_bytes));
        let names: Vec<String> = archive
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

        assert!(names.contains(&"cache/sub/item".to_string()));
    }

    #[test]
    fn archive_rule_skips_real_file_colliding_with_archive_path() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir(source_dir.path().join("cache")).unwrap();
        fs::write(source_dir.path().join("cache/item"), b"cached").unwrap();
        fs::write(source_dir.path().join("cache.tar"), b"real cache tar").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                rules: vec![ScanRule::new(r"(^|/)cache$", ScanRuleAction::Archive)],
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.skipped_files, 1);

        let meta = metadata_db
            .get_file("/repo/cache.tar")
            .unwrap()
            .expect("archive metadata should exist");
        let cid = cid_util::cid_from_bytes(&meta.cid).unwrap();
        let bytes = content_store.read(&cid).unwrap();
        assert_ne!(bytes, b"real cache tar");
    }

    #[test]
    fn archive_rule_preserves_empty_directory_root_entry() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir(source_dir.path().join("cache")).unwrap();

        scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                rules: vec![ScanRule::new(r"(^|/)cache$", ScanRuleAction::Archive)],
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();

        let archive_meta = metadata_db
            .get_file("/repo/cache.tar")
            .unwrap()
            .expect("expected cache.tar metadata");
        let archive_cid = cid_util::cid_from_bytes(&archive_meta.cid).unwrap();
        let archive_bytes = content_store.read(&archive_cid).unwrap();
        let mut archive = tar::Archive::new(std::io::Cursor::new(archive_bytes));
        let entries: Vec<(String, tar::EntryType)> = archive
            .entries()
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                let path = entry.path().unwrap().to_string_lossy().to_string();
                let entry_type = entry.header().entry_type();
                (path, entry_type)
            })
            .collect();

        assert!(entries.iter().any(|(path, entry_type)| {
            (path == "cache" || path == "cache/") && *entry_type == tar::EntryType::Directory
        }));
    }

    #[cfg(unix)]
    #[test]
    fn archive_rule_preserves_backslashes_inside_path_components() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir(source_dir.path().join("cache")).unwrap();
        fs::write(source_dir.path().join("cache").join(r"a\b"), b"cached").unwrap();

        scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                rules: vec![ScanRule::new(r"(^|/)cache$", ScanRuleAction::Archive)],
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();

        let archive_meta = metadata_db
            .get_file("/repo/cache.tar")
            .unwrap()
            .expect("expected cache.tar metadata");
        let archive_cid = cid_util::cid_from_bytes(&archive_meta.cid).unwrap();
        let archive_bytes = content_store.read(&archive_cid).unwrap();
        let mut archive = tar::Archive::new(std::io::Cursor::new(archive_bytes));
        let names: Vec<String> = archive
            .entries()
            .unwrap()
            .map(|entry| entry.unwrap().path().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(names.contains(&r"cache/a\b".to_string()));
        assert!(!names.contains(&"cache/a/b".to_string()));
    }

    #[test]
    fn scan_rules_do_not_match_scan_root() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("file.txt"), b"content").unwrap();

        scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                rules: vec![ScanRule::new(r"^$", ScanRuleAction::Archive)],
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();

        assert!(metadata_db.get_file("/repo.tar").unwrap().is_none());
        assert!(metadata_db.get_file("/repo/file.txt").unwrap().is_some());
    }

    #[test]
    fn archive_collision_does_not_treat_root_tar_as_archived_root() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join(".tar"), b"root tar").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                rules: vec![ScanRule::new(r"^$", ScanRuleAction::Archive)],
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.skipped_files, 0);

        let meta = metadata_db
            .get_file("/repo/.tar")
            .unwrap()
            .expect("root .tar file should be stored");
        let cid = cid_util::cid_from_bytes(&meta.cid).unwrap();
        let bytes = content_store.read(&cid).unwrap();
        assert_eq!(bytes, b"root tar");
    }

    #[test]
    fn archive_collision_does_not_match_trailing_slash_synthetic_path() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir(source_dir.path().join("foo")).unwrap();
        fs::write(source_dir.path().join("foo/.tar"), b"nested tar").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                rules: vec![ScanRule::new(r"^foo/$", ScanRuleAction::Archive)],
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.skipped_files, 0);

        let meta = metadata_db
            .get_file("/repo/foo/.tar")
            .unwrap()
            .expect("nested .tar file should be stored");
        let cid = cid_util::cid_from_bytes(&meta.cid).unwrap();
        let bytes = content_store.read(&cid).unwrap();
        assert_eq!(bytes, b"nested tar");
    }

    #[test]
    fn archive_rule_matching_regular_file_skips_it() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("cache"), b"not a directory").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                rules: vec![ScanRule::new(r"(^|/)cache$", ScanRuleAction::Archive)],
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.skipped_files, 1);
        assert!(metadata_db.get_file("/repo/cache").unwrap().is_none());
        assert!(metadata_db.get_file("/repo/cache.tar").unwrap().is_none());
    }

    #[test]
    fn scan_rules_use_first_match() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir_all(source_dir.path().join("cache/sub")).unwrap();
        fs::write(source_dir.path().join("cache/sub/item"), b"cached").unwrap();
        fs::write(source_dir.path().join("cache.tar"), b"real cache tar").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                rules: vec![
                    ScanRule::new(r"(^|/)cache$", ScanRuleAction::Ignore),
                    ScanRule::new(r"(^|/)cache$", ScanRuleAction::Archive),
                ],
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.skipped_files, 1);
        assert!(metadata_db
            .get_file("/repo/cache/sub/item")
            .unwrap()
            .is_none());

        let meta = metadata_db
            .get_file("/repo/cache.tar")
            .unwrap()
            .expect("real cache.tar file should be stored");
        let cid = cid_util::cid_from_bytes(&meta.cid).unwrap();
        let bytes = content_store.read(&cid).unwrap();
        assert_eq!(bytes, b"real cache tar");
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
                prune_deleted: false,
                parallelism: None,
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
                prune_deleted: false,
                parallelism: None,
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
                prune_deleted: false,
                parallelism: None,
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
                prune_deleted: false,
                parallelism: None,
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
                prune_deleted: false,
                parallelism: None,
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
        let result = build_directory_archive(".git", &git_dir, &|| {
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
                prune_deleted: false,
                parallelism: None,
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
        assert_eq!(files.len(), 0);
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

    #[test]
    fn rescan_skips_unchanged_files() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("a.txt"), b"alpha").unwrap();
        fs::write(source_dir.path().join("b.txt"), b"beta").unwrap();

        let first = scan_directory(
            source_dir.path(),
            store_dir.path(),
            &content_store,
            &metadata_db,
        )
        .unwrap();
        assert_eq!(first.total_files, 2);
        assert_eq!(first.unchanged_files, 0);
        assert_eq!(first.unique_blobs, 2);

        let second = scan_directory(
            source_dir.path(),
            store_dir.path(),
            &content_store,
            &metadata_db,
        )
        .unwrap();

        assert_eq!(second.total_files, 2);
        assert_eq!(second.unchanged_files, 2);
        assert_eq!(second.unique_blobs, 0);
        assert_eq!(second.duplicate_files, 0);
    }

    #[test]
    fn rescan_reprocesses_file_with_changed_size() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("a.txt"), b"short").unwrap();
        scan_directory(
            source_dir.path(),
            store_dir.path(),
            &content_store,
            &metadata_db,
        )
        .unwrap();

        let new_content = b"a much longer content string";
        fs::write(source_dir.path().join("a.txt"), new_content).unwrap();

        let second = scan_directory(
            source_dir.path(),
            store_dir.path(),
            &content_store,
            &metadata_db,
        )
        .unwrap();

        assert_eq!(second.unchanged_files, 0);
        assert_eq!(second.total_files, 1);

        let meta = metadata_db.get_file("/a.txt").unwrap().unwrap();
        assert_eq!(meta.original_size, new_content.len() as u64);
    }

    #[test]
    fn rescan_reprocesses_when_blob_missing() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("a.txt"), b"payload").unwrap();
        scan_directory(
            source_dir.path(),
            store_dir.path(),
            &content_store,
            &metadata_db,
        )
        .unwrap();

        let blobs_dir = store_dir.path().join("blobs");
        for entry in fs::read_dir(&blobs_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                fs::remove_file(path).unwrap();
            }
        }

        let second = scan_directory(
            source_dir.path(),
            store_dir.path(),
            &content_store,
            &metadata_db,
        )
        .unwrap();

        assert_eq!(second.unchanged_files, 0);
        assert_eq!(second.unique_blobs, 1);

        let meta = metadata_db.get_file("/a.txt").unwrap().unwrap();
        let cid = cid_util::cid_from_bytes(&meta.cid).unwrap();
        assert!(content_store.exists(&cid));
    }

    #[test]
    fn unchanged_file_metadata_detects_size_and_mtime_changes() {
        let (_source_dir, _store_dir, content_store, metadata_db) = setup_test_store();

        let data = b"deterministic payload";
        let cid = cid_util::compute_cid(data);
        content_store.store(&cid, data).unwrap();

        let meta = FileMetadata {
            cid: cid_util::cid_to_bytes(&cid),
            original_size: data.len() as u64,
            compressed_size: 0,
            modified: 1_000,
            created: 0,
            permissions: 0o644,
        };
        metadata_db
            .insert_file("/x.bin", &meta, &cid_util::cid_to_string(&cid))
            .unwrap();

        let size = data.len() as u64;
        assert!(
            unchanged_file_metadata(&metadata_db, &content_store, "/x.bin", size, 1_000).is_some()
        );
        assert!(
            unchanged_file_metadata(&metadata_db, &content_store, "/x.bin", size, 2_000).is_none()
        );
        assert!(
            unchanged_file_metadata(&metadata_db, &content_store, "/x.bin", size + 1, 1_000)
                .is_none()
        );
        assert!(
            unchanged_file_metadata(&metadata_db, &content_store, "/missing", size, 1_000)
                .is_none()
        );
    }

    #[test]
    fn scan_without_prune_preserves_deleted_entries() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("a.txt"), b"alpha").unwrap();
        fs::write(source_dir.path().join("b.txt"), b"beta").unwrap();
        scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions::default(),
            |_| {},
        )
        .unwrap();

        fs::remove_file(source_dir.path().join("b.txt")).unwrap();

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

        assert_eq!(stats.pruned_entries, 0);
        assert!(metadata_db.get_file("/repo/a.txt").unwrap().is_some());
        assert!(metadata_db.get_file("/repo/b.txt").unwrap().is_some());
    }

    #[test]
    fn prune_removes_deleted_files() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("a.txt"), b"alpha").unwrap();
        fs::write(source_dir.path().join("b.txt"), b"beta").unwrap();
        let opts = ScanOptions {
            prune_deleted: true,
            ..ScanOptions::default()
        };
        scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            opts.clone(),
            |_| {},
        )
        .unwrap();

        fs::remove_file(source_dir.path().join("b.txt")).unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            opts,
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.pruned_entries, 1);
        assert!(metadata_db.get_file("/repo/a.txt").unwrap().is_some());
        assert!(metadata_db.get_file("/repo/b.txt").unwrap().is_none());

        let names: Vec<String> = metadata_db
            .list_dir("/repo")
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert!(names.contains(&"a.txt".to_string()));
        assert!(!names.contains(&"b.txt".to_string()));
    }

    #[test]
    fn prune_removes_deleted_directory_and_children() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir_all(source_dir.path().join("sub")).unwrap();
        fs::write(source_dir.path().join("sub/inner.txt"), b"inner").unwrap();
        fs::write(source_dir.path().join("keep.txt"), b"keep").unwrap();
        let opts = ScanOptions {
            prune_deleted: true,
            ..ScanOptions::default()
        };
        scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            opts.clone(),
            |_| {},
        )
        .unwrap();
        assert!(metadata_db
            .get_file("/repo/sub/inner.txt")
            .unwrap()
            .is_some());

        fs::remove_dir_all(source_dir.path().join("sub")).unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            opts,
            |_| {},
        )
        .unwrap();

        assert!(stats.pruned_entries >= 2);
        assert!(metadata_db
            .get_file("/repo/sub/inner.txt")
            .unwrap()
            .is_none());
        assert!(metadata_db.get_file("/repo/keep.txt").unwrap().is_some());

        let repo_names: Vec<String> = metadata_db
            .list_dir("/repo")
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert!(!repo_names.contains(&"sub".to_string()));
    }

    #[test]
    fn prune_is_scoped_to_target_prefix() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::write(source_dir.path().join("a.txt"), b"a").unwrap();
        fs::write(source_dir.path().join("b.txt"), b"b").unwrap();
        let opts = ScanOptions {
            prune_deleted: true,
            ..ScanOptions::default()
        };
        scan_directory_into_with_options(
            source_dir.path(),
            "/one",
            store_dir.path(),
            &content_store,
            &metadata_db,
            opts.clone(),
            |_| {},
        )
        .unwrap();

        let other = TempDir::new().unwrap();
        fs::write(other.path().join("c.txt"), b"c").unwrap();
        scan_directory_into_with_options(
            other.path(),
            "/two",
            store_dir.path(),
            &content_store,
            &metadata_db,
            opts.clone(),
            |_| {},
        )
        .unwrap();

        fs::remove_file(source_dir.path().join("a.txt")).unwrap();
        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/one",
            store_dir.path(),
            &content_store,
            &metadata_db,
            opts,
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.pruned_entries, 1);
        assert!(metadata_db.get_file("/one/a.txt").unwrap().is_none());
        assert!(metadata_db.get_file("/one/b.txt").unwrap().is_some());
        assert!(metadata_db.get_file("/two/c.txt").unwrap().is_some());
    }
}
