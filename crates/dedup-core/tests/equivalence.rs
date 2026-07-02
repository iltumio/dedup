use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use dedup_core::{
    BuiltinScanPreset, FileMetadata, ScanOptions, ScanRule, ScanRuleAction, ScanStats, Store,
};
use tempfile::TempDir;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ComparableFileMetadata {
    cid: Vec<u8>,
    original_size: u64,
    compressed_size: u64,
    modified: i64,
    created: i64,
    permissions: u32,
}

impl From<FileMetadata> for ComparableFileMetadata {
    fn from(meta: FileMetadata) -> Self {
        Self {
            cid: meta.cid,
            original_size: meta.original_size,
            compressed_size: meta.compressed_size,
            modified: meta.modified,
            created: meta.created,
            permissions: meta.permissions,
        }
    }
}

struct StoreSnapshot {
    files: BTreeMap<String, ComparableFileMetadata>,
    dirs: BTreeSet<String>,
    blobs: BTreeMap<String, Vec<u8>>,
}

struct EquivalenceRun {
    _source_dir: TempDir,
    serial_store_dir: TempDir,
    parallel_store_dir: TempDir,
    serial_stats: ScanStats,
    parallel_stats: ScanStats,
}

fn assert_scan_equivalent(build_tree: impl Fn(&Path), base_options: ScanOptions) {
    let _run = run_scan_equivalent(build_tree, base_options);
}

fn run_scan_equivalent(build_tree: impl Fn(&Path), base_options: ScanOptions) -> EquivalenceRun {
    let source_dir = TempDir::new().unwrap();
    let serial_store_dir = TempDir::new().unwrap();
    let parallel_store_dir = TempDir::new().unwrap();

    build_tree(source_dir.path());

    let serial_store = Store::open(serial_store_dir.path()).unwrap();
    let parallel_store = Store::open(parallel_store_dir.path()).unwrap();

    let serial_stats = scan_with_parallelism(&serial_store, source_dir.path(), &base_options, 1);
    let parallel_stats =
        scan_with_parallelism(&parallel_store, source_dir.path(), &base_options, 8);

    assert_stores_equivalent(
        &serial_store,
        serial_store_dir.path(),
        &parallel_store,
        parallel_store_dir.path(),
    );
    assert_stats_equivalent(&serial_stats, &parallel_stats);

    EquivalenceRun {
        _source_dir: source_dir,
        serial_store_dir,
        parallel_store_dir,
        serial_stats,
        parallel_stats,
    }
}

fn scan_with_parallelism(
    store: &Store,
    source: &Path,
    base_options: &ScanOptions,
    parallelism: usize,
) -> ScanStats {
    let mut options = base_options.clone();
    options.parallelism = Some(parallelism);
    store
        .scan_into_with_options(source, "/", options, |_| {})
        .unwrap()
}

fn assert_stores_equivalent(
    serial_store: &Store,
    serial_root: &Path,
    parallel_store: &Store,
    parallel_root: &Path,
) {
    let serial_snapshot = snapshot_store(serial_store, serial_root);
    let parallel_snapshot = snapshot_store(parallel_store, parallel_root);

    assert_eq!(serial_snapshot.files, parallel_snapshot.files);
    assert_eq!(serial_snapshot.dirs, parallel_snapshot.dirs);
    assert_eq!(serial_snapshot.blobs, parallel_snapshot.blobs);
}

fn snapshot_store(store: &Store, store_root: &Path) -> StoreSnapshot {
    let mut files = BTreeMap::new();
    let mut dirs = BTreeSet::from(["/".to_string()]);
    walk_virtual_dir(store, "/", &mut files, &mut dirs);

    StoreSnapshot {
        files,
        dirs,
        blobs: blob_contents(store, store_root),
    }
}

fn walk_virtual_dir(
    store: &Store,
    dir: &str,
    files: &mut BTreeMap<String, ComparableFileMetadata>,
    dirs: &mut BTreeSet<String>,
) {
    let mut entries = store.list_dir(dir).unwrap();
    entries.sort_by(|left, right| left.name.cmp(&right.name));

    for entry in entries {
        let path = if dir == "/" {
            format!("/{}", entry.name)
        } else {
            format!("{dir}/{}", entry.name)
        };

        if entry.is_dir {
            dirs.insert(path.clone());
            walk_virtual_dir(store, path.as_str(), files, dirs);
        } else {
            let metadata = store.get_file(path.as_str()).unwrap().unwrap();
            files.insert(path, metadata.into());
        }
    }
}

fn blob_contents(store: &Store, store_root: &Path) -> BTreeMap<String, Vec<u8>> {
    let blobs_dir = store_root.join("blobs");
    let mut blobs = BTreeMap::new();

    for entry in fs::read_dir(blobs_dir).unwrap() {
        let entry = entry.unwrap();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.ends_with(".lz4") {
            continue;
        }

        let cid_string = file_name.trim_end_matches(".lz4");
        let cid = dedup_core::cid::cid_from_string(cid_string).unwrap();
        blobs.insert(file_name, store.content.read(&cid).unwrap());
    }

    blobs
}

fn assert_stats_equivalent(left: &ScanStats, right: &ScanStats) {
    assert_eq!(left.total_files, right.total_files);
    assert_eq!(left.total_dirs, right.total_dirs);
    assert_eq!(left.unique_blobs, right.unique_blobs);
    assert_eq!(left.duplicate_files, right.duplicate_files);
    assert_eq!(left.total_original_bytes, right.total_original_bytes);
    assert_eq!(left.total_stored_bytes, right.total_stored_bytes);
    assert_eq!(left.skipped_files, right.skipped_files);
    assert_eq!(left.unchanged_files, right.unchanged_files);
    assert_eq!(left.pruned_entries, right.pruned_entries);
    assert_eq!(
        left.errors_log_path.is_some(),
        right.errors_log_path.is_some()
    );
}

fn write_file(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) {
    fs::write(path, contents).unwrap();
}

fn create_dir(path: impl AsRef<Path>) {
    fs::create_dir_all(path).unwrap();
}

fn add_parallel_filler(root: &Path) {
    create_dir(root.join("parallel-filler"));
    for index in 0..70 {
        write_file(
            root.join(format!("parallel-filler/{index:02}.txt")),
            format!("parallel filler {index}"),
        );
    }
}

fn assert_no_tmp_blobs(store_root: &Path) {
    let leftovers: Vec<String> = fs::read_dir(store_root.join("blobs"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".tmp"))
        .collect();

    assert!(leftovers.is_empty(), "leftover temp blobs: {leftovers:?}");
}

#[test]
fn simple_files_and_subdirs() {
    assert_scan_equivalent(
        |root| {
            add_parallel_filler(root);
            create_dir(root.join("docs/sub"));
            write_file(root.join("hello.txt"), b"hello");
            write_file(root.join("docs/readme.md"), b"readme");
            write_file(root.join("docs/sub/data.bin"), [0, 1, 2, 3, 4]);
        },
        ScanOptions::default(),
    );
}

#[test]
fn many_duplicate_content_files() {
    assert_scan_equivalent(
        |root| {
            create_dir(root.join("dupes"));
            for index in 0..75 {
                write_file(
                    root.join(format!("dupes/copy-{index}.txt")),
                    b"same content",
                );
            }
            write_file(root.join("unique.txt"), b"unique content");
        },
        ScanOptions::default(),
    );
}

#[test]
fn ignore_scan_rule_dir() {
    assert_scan_equivalent(
        |root| {
            add_parallel_filler(root);
            create_dir(root.join("target/debug"));
            write_file(root.join("target/debug/app"), b"ignored binary");
            write_file(root.join("src.rs"), b"kept source");
        },
        ScanOptions {
            rules: vec![ScanRule::new(r"(^|/)target$", ScanRuleAction::Ignore)],
            ..ScanOptions::default()
        },
    );
}

#[test]
fn archive_scan_rule_dir() {
    assert_scan_equivalent(
        |root| {
            add_parallel_filler(root);
            create_dir(root.join("cache/sub"));
            write_file(root.join("cache/sub/item"), b"cached");
            write_file(root.join("keep.txt"), b"keep");
        },
        ScanOptions {
            rules: vec![ScanRule::new(r"(^|/)cache$", ScanRuleAction::Archive)],
            ..ScanOptions::default()
        },
    );
}

#[test]
fn bundle_git_dirs_with_git_dir() {
    assert_scan_equivalent(
        |root| {
            add_parallel_filler(root);
            create_dir(root.join("repo/.git/objects/ab"));
            write_file(root.join("repo/.git/HEAD"), b"ref: refs/heads/main\n");
            write_file(root.join("repo/.git/objects/ab/object"), b"git object");
            write_file(root.join("repo/src.rs"), b"fn main() {}\n");
        },
        ScanOptions {
            bundle_git_dirs: true,
            ..ScanOptions::default()
        },
    );
}

#[test]
fn rescan_unchanged_same_store_twice() {
    let source_dir = TempDir::new().unwrap();
    add_parallel_filler(source_dir.path());
    create_dir(source_dir.path().join("nested"));
    write_file(source_dir.path().join("nested/a.txt"), b"alpha");
    write_file(source_dir.path().join("b.txt"), b"beta");

    let serial_store_dir = TempDir::new().unwrap();
    let parallel_store_dir = TempDir::new().unwrap();
    let serial_store = Store::open(serial_store_dir.path()).unwrap();
    let parallel_store = Store::open(parallel_store_dir.path()).unwrap();
    let base_options = ScanOptions::default();

    scan_with_parallelism(&serial_store, source_dir.path(), &base_options, 1);
    scan_with_parallelism(&parallel_store, source_dir.path(), &base_options, 8);
    thread::sleep(Duration::from_secs(1));
    write_file(source_dir.path().join("nested/a.txt"), b"alpha");
    let serial_stats = scan_with_parallelism(&serial_store, source_dir.path(), &base_options, 1);
    let parallel_stats =
        scan_with_parallelism(&parallel_store, source_dir.path(), &base_options, 8);

    assert_stores_equivalent(
        &serial_store,
        serial_store_dir.path(),
        &parallel_store,
        parallel_store_dir.path(),
    );
    assert_stats_equivalent(&serial_stats, &parallel_stats);
}

#[test]
fn prune_deleted_file() {
    let source_dir = TempDir::new().unwrap();
    add_parallel_filler(source_dir.path());
    create_dir(source_dir.path().join("data"));
    let removed = source_dir.path().join("data/remove.txt");
    write_file(&removed, b"remove me");
    write_file(source_dir.path().join("data/keep.txt"), b"keep me");

    let serial_store_dir = TempDir::new().unwrap();
    let parallel_store_dir = TempDir::new().unwrap();
    let serial_store = Store::open(serial_store_dir.path()).unwrap();
    let parallel_store = Store::open(parallel_store_dir.path()).unwrap();
    let mut base_options = ScanOptions::default();

    scan_with_parallelism(&serial_store, source_dir.path(), &base_options, 1);
    scan_with_parallelism(&parallel_store, source_dir.path(), &base_options, 8);

    fs::remove_file(removed).unwrap();
    base_options.prune_deleted = true;

    let serial_stats = scan_with_parallelism(&serial_store, source_dir.path(), &base_options, 1);
    let parallel_stats =
        scan_with_parallelism(&parallel_store, source_dir.path(), &base_options, 8);

    assert_stores_equivalent(
        &serial_store,
        serial_store_dir.path(),
        &parallel_store,
        parallel_store_dir.path(),
    );
    assert_stats_equivalent(&serial_stats, &parallel_stats);
    assert!(serial_store.get_file("/data/remove.txt").unwrap().is_none());
    assert!(parallel_store
        .get_file("/data/remove.txt")
        .unwrap()
        .is_none());
}

#[test]
fn empty_dir() {
    assert_scan_equivalent(|_root| {}, ScanOptions::default());
}

#[test]
fn deep_nested_dirs() {
    assert_scan_equivalent(
        |root| {
            add_parallel_filler(root);
            let mut current = PathBuf::from(root);
            for depth in 0..25 {
                current.push(format!("level-{depth}"));
                create_dir(&current);
                write_file(
                    current.join(format!("file-{depth}.txt")),
                    format!("depth {depth}"),
                );
            }
        },
        ScanOptions::default(),
    );
}

#[cfg(unix)]
#[test]
fn read_error_file() {
    use std::os::unix::fs::PermissionsExt;

    assert_scan_equivalent(
        |root| {
            add_parallel_filler(root);
            write_file(root.join("readable.txt"), b"readable");
            let unreadable = root.join("unreadable.txt");
            write_file(&unreadable, b"unreadable");
            let mut permissions = fs::metadata(&unreadable).unwrap().permissions();
            permissions.set_mode(0o000);
            fs::set_permissions(unreadable, permissions).unwrap();
        },
        ScanOptions::default(),
    );
}

#[test]
fn builtin_preset_ignore_dir() {
    assert_scan_equivalent(
        |root| {
            add_parallel_filler(root);
            create_dir(root.join("node_modules/pkg"));
            write_file(root.join("node_modules/pkg/index.js"), b"ignored");
            write_file(root.join("package.json"), br#"{"name":"app"}"#);
        },
        ScanOptions {
            rules: vec![ScanRule::builtin(BuiltinScanPreset::NodeModules)],
            ..ScanOptions::default()
        },
    );
}

#[test]
fn same_cid_stress() {
    let run = run_scan_equivalent(
        |root| {
            create_dir(root.join("copies"));
            for index in 0..300 {
                write_file(
                    root.join(format!("copies/{index:03}.txt")),
                    b"identical payload",
                );
            }
        },
        ScanOptions::default(),
    );

    assert_eq!(run.serial_stats.unique_blobs, 1);
    assert_eq!(run.parallel_stats.unique_blobs, 1);
    assert_eq!(run.serial_stats.duplicate_files, 299);
    assert_eq!(run.parallel_stats.duplicate_files, 299);
    assert_no_tmp_blobs(run.serial_store_dir.path());
    assert_no_tmp_blobs(run.parallel_store_dir.path());
}

#[test]
fn cancellation_does_not_hang() {
    let source_dir = TempDir::new().unwrap();
    create_dir(source_dir.path().join("many"));
    for index in 0..1_000 {
        write_file(
            source_dir.path().join(format!("many/file-{index:04}.txt")),
            format!("payload {index}"),
        );
    }

    let store_dir = TempDir::new().unwrap();
    let source_path = source_dir.path().to_path_buf();
    let store_path = store_dir.path().to_path_buf();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let store = Store::open(&store_path).unwrap();
        let progress_count = Arc::new(AtomicUsize::new(0));
        let cancel_count = Arc::clone(&progress_count);
        let result = store.scan_into_with_options_and_cancellation(
            &source_path,
            "/",
            ScanOptions {
                parallelism: Some(8),
                ..ScanOptions::default()
            },
            |progress| {
                if progress.files_processed > 0 {
                    progress_count.store(progress.files_processed as usize, Ordering::SeqCst);
                }
            },
            || cancel_count.load(Ordering::SeqCst) >= 1,
        );
        tx.send(result.map(|_| ())).unwrap();
    });

    let deadline = Instant::now() + Duration::from_secs(30);
    let result = loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() < deadline => continue,
            Err(mpsc::RecvTimeoutError::Timeout) => panic!("cancelled scan did not finish in time"),
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!("scan thread disconnected"),
        }
    };

    assert!(result.is_err(), "cancelled scan unexpectedly succeeded");
}

#[test]
fn incremental_rescan_many_files_mixed_changes() {
    let source_dir = TempDir::new().unwrap();
    for dir_index in 0..12 {
        let dir = source_dir.path().join(format!("bulk/dir{dir_index:02}"));
        create_dir(&dir);
        for file_index in 0..40 {
            write_file(
                dir.join(format!("f{file_index:02}.txt")),
                format!("content {dir_index} {file_index}"),
            );
        }
    }

    let serial_store_dir = TempDir::new().unwrap();
    let parallel_store_dir = TempDir::new().unwrap();
    let serial_store = Store::open(serial_store_dir.path()).unwrap();
    let parallel_store = Store::open(parallel_store_dir.path()).unwrap();
    let mut base_options = ScanOptions::default();

    scan_with_parallelism(&serial_store, source_dir.path(), &base_options, 1);
    scan_with_parallelism(&parallel_store, source_dir.path(), &base_options, 8);
    thread::sleep(Duration::from_secs(1));

    for dir_index in 0..12 {
        let dir = source_dir.path().join(format!("bulk/dir{dir_index:02}"));
        write_file(dir.join("f00.txt"), format!("changed {dir_index}"));
        write_file(dir.join("new.txt"), format!("new {dir_index}"));
        fs::remove_file(dir.join("f01.txt")).unwrap();
    }
    base_options.prune_deleted = true;

    let serial_stats = scan_with_parallelism(&serial_store, source_dir.path(), &base_options, 1);
    let parallel_stats =
        scan_with_parallelism(&parallel_store, source_dir.path(), &base_options, 8);

    assert_stores_equivalent(
        &serial_store,
        serial_store_dir.path(),
        &parallel_store,
        parallel_store_dir.path(),
    );
    assert_stats_equivalent(&serial_stats, &parallel_stats);
    // 480 originals - 12 modified - 12 deleted = 456 untouched files must be
    // detected as unchanged by both paths.
    assert_eq!(serial_stats.unchanged_files, 456);
    assert_eq!(parallel_stats.unchanged_files, 456);
    assert_eq!(serial_stats.pruned_entries, 12);
    assert_eq!(parallel_stats.pruned_entries, 12);
}
