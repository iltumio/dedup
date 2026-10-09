use std::fs;
use std::path::Path;
use std::sync::Mutex;

use dedup_core::{cid, migration, ChunkingProfile, ContentStore, ScanOptions, Store};
use tempfile::TempDir;

struct SyncCell<T>(Mutex<T>);
impl<T: Copy> SyncCell<T> {
    fn new(value: T) -> Self {
        Self(Mutex::new(value))
    }
    fn get(&self) -> T {
        *self.0.lock().unwrap()
    }
    fn set(&self, value: T) {
        *self.0.lock().unwrap() = value;
    }
}
fn migrate_serial<F, C>(
    source: &Path,
    destination: &Path,
    profile: ChunkingProfile,
    progress: F,
    cancel: C,
) -> anyhow::Result<migration::MigrationProgress>
where
    F: Fn(&migration::MigrationProgress) + Sync,
    C: Fn() -> bool + Sync,
{
    migration::migrate_with_options(
        source,
        destination,
        profile,
        migration::MigrationOptions { workers: 1 },
        progress,
        cancel,
    )
}

fn payload(size: usize) -> Vec<u8> {
    let mut seed = 0x123456789abcdefu64;
    (0..size)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as u8
        })
        .collect()
}

fn legacy_archive(root: &Path, source: &Path) {
    fs::create_dir(root.join("blobs")).unwrap();
    let store = Store::open(root).unwrap();
    assert_eq!(store.content.format_version(), 1);
    store
        .scan_into_with_options(
            source,
            "/backup",
            ScanOptions {
                parallelism: Some(1),
                ..Default::default()
            },
            |_| {},
        )
        .unwrap();
}

fn blobs(root: &Path) -> Vec<std::path::PathBuf> {
    walkdir::WalkDir::new(root.join("blobs"))
        .follow_links(false)
        .into_iter()
        .map(|e| e.unwrap().into_path())
        .filter(|p| p.extension().is_some_and(|e| e == "lz4"))
        .collect()
}

#[test]
fn fastcdc_blobs_use_digest_shards() {
    let root = TempDir::new().unwrap();
    let store = ContentStore::open(root.path()).unwrap();
    let data = payload(4 * 1024 * 1024);
    let cid = cid::compute_cid(&data);
    store.store(&cid, &data).unwrap();
    let chunks = blobs(root.path());
    assert!(chunks.len() > 8);
    for path in chunks {
        let name = path.file_stem().unwrap().to_str().unwrap();
        let chunk = cid::cid_from_string(name).unwrap();
        let digest = chunk.hash().digest();
        let shard = format!("{:02x}/{:x}", digest[0], digest[1] >> 4);
        assert_eq!(
            path.parent().unwrap(),
            root.path().join("blobs").join(shard)
        );
    }
    assert_eq!(store.read(&cid).unwrap(), data);
}

#[test]
fn migration_roundtrip_on_configurable_filesystem() {
    let source = TempDir::new().unwrap();
    let legacy = TempDir::new().unwrap();
    let target = match std::env::var_os("DEDUP_TEST_FS_ROOT") {
        Some(root) => TempDir::new_in(root).unwrap(),
        None => TempDir::new().unwrap(),
    };
    let bytes = payload(1024 * 1024);
    fs::write(source.path().join("a"), &bytes).unwrap();
    fs::write(source.path().join("b"), b"another file").unwrap();
    legacy_archive(legacy.path(), source.path());
    let original_metadata = fs::read(legacy.path().join("metadata.redb")).unwrap();
    let cancelled = SyncCell::new(false);
    let result = migrate_serial(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        |p| cancelled.set(p.unique_files == 1),
        || cancelled.get(),
    );
    let error = format!("{:#}", result.unwrap_err());
    assert!(
        error.contains("migration cancelled"),
        "unexpected error: {error}"
    );
    let progress = migrate_serial(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(progress.resumed_files, 1);
    assert_eq!(progress.unique_files, 2);
    let migrated = Store::open_existing(target.path()).unwrap();
    assert_eq!(migrated.read_file("/backup/a").unwrap(), bytes);
    assert_eq!(migrated.read_file("/backup/b").unwrap(), b"another file");
    assert_eq!(
        fs::read(legacy.path().join("metadata.redb")).unwrap(),
        original_metadata
    );
}

#[test]
fn migration_into_a_dedicated_subdirectory_of_disk_root_preserves_source() {
    let disk = TempDir::new().unwrap();
    let input = TempDir::new().unwrap();
    fs::write(input.path().join("file"), b"disk-root archive").unwrap();
    legacy_archive(disk.path(), input.path());
    let before_metadata = fs::read(disk.path().join("metadata.redb")).unwrap();
    let before_blobs = blobs(disk.path())
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect::<Vec<_>>();
    let destination = disk.path().join("fastcdc");
    let cancelled = SyncCell::new(false);
    let stopped = migrate_serial(
        disk.path(),
        &destination,
        ChunkingProfile::default(),
        |p| cancelled.set(p.unique_files == 1),
        || cancelled.get(),
    );
    assert!(stopped
        .unwrap_err()
        .to_string()
        .contains("migration cancelled"));
    assert!(Store::open_existing(&destination).is_err());
    let resumed = migrate_serial(
        disk.path(),
        &destination,
        ChunkingProfile::default(),
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(resumed.resumed_files, 1);
    assert_eq!(
        Store::open_existing(&destination)
            .unwrap()
            .read_file("/backup/file")
            .unwrap(),
        b"disk-root archive"
    );
    assert_eq!(
        fs::read(disk.path().join("metadata.redb")).unwrap(),
        before_metadata
    );
    for (path, bytes) in before_blobs {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    let source = Store::open_existing(disk.path()).unwrap();
    assert_eq!(source.content.format_version(), 1);
    assert_eq!(
        source.read_file("/backup/file").unwrap(),
        b"disk-root archive"
    );
}

#[test]
fn migration_can_stop_inside_a_large_file_and_resume() {
    let source = TempDir::new().unwrap();
    let legacy = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let bytes = payload(4 * 1024 * 1024);
    fs::write(source.path().join("large"), &bytes).unwrap();
    legacy_archive(legacy.path(), source.path());
    let checks = SyncCell::new(0);
    let verified = SyncCell::new(0);
    let result = migrate_serial(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        |progress| verified.set(progress.unique_files),
        || {
            checks.set(checks.get() + 1);
            checks.get() >= 5
        },
    );
    assert!(format!("{:#}", result.unwrap_err()).contains("migration cancelled"));
    assert_eq!(verified.get(), 0);
    assert!(!blobs(target.path()).is_empty());
    assert!(Store::open(target.path()).is_err());
    let result = migrate_serial(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(result.unique_files, 1);
    assert_eq!(
        Store::open(target.path())
            .unwrap()
            .read_file("/backup/large")
            .unwrap(),
        bytes
    );
}

#[test]
fn migration_resumes_flat_v2_chunks_without_relocating_the_entire_archive() {
    #[derive(serde::Serialize)]
    struct OldFormat {
        version: u32,
        profile: ChunkingProfile,
    }
    let input = TempDir::new().unwrap();
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let data = payload(4 * 1024 * 1024);
    fs::write(input.path().join("large"), &data).unwrap();
    legacy_archive(source.path(), input.path());
    let checks = SyncCell::new(0);
    assert!(migrate_serial(
        source.path(),
        target.path(),
        ChunkingProfile::default(),
        |_| {},
        || {
            checks.set(checks.get() + 1);
            checks.get() >= 5
        }
    )
    .is_err());
    // Reproduce the layout/checkpoint written by a pre-optimization release.
    let previous = blobs(target.path());
    assert!(!previous.is_empty());
    for path in &previous {
        fs::rename(
            path,
            target.path().join("blobs").join(path.file_name().unwrap()),
        )
        .unwrap();
    }
    let db = redb::Database::open(target.path().join("content.redb")).unwrap();
    let txn = db.begin_write().unwrap();
    let definition: redb::TableDefinition<&str, &[u8]> = redb::TableDefinition::new("config");
    let encoded = bincode::serialize(&OldFormat {
        version: 2,
        profile: ChunkingProfile::default(),
    })
    .unwrap();
    txn.open_table(definition)
        .unwrap()
        .insert("format", encoded.as_slice())
        .unwrap();
    txn.commit().unwrap();
    drop(db);
    let result = migrate_serial(
        source.path(),
        target.path(),
        ChunkingProfile::default(),
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(result.unique_files, 1);
    let flat_count = fs::read_dir(target.path().join("blobs"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|e| e == "lz4")
        })
        .count();
    assert_eq!(flat_count, previous.len());
    let migrated = Store::open_existing(target.path()).unwrap();
    assert_eq!(migrated.content.format_version(), 3);
    assert_eq!(migrated.read_file("/backup/large").unwrap(), data);
    assert_eq!(
        blobs(target.path()).len(),
        fastcdc::v2020::FastCDC::with_level(
            &data,
            16384,
            65536,
            262144,
            fastcdc::v2020::Normalization::Level1
        )
        .count()
    );
}

#[test]
fn shifted_files_share_chunks_and_keep_whole_file_cids() {
    let root = TempDir::new().unwrap();
    let store = ContentStore::open(root.path()).unwrap();
    assert_eq!(store.format_version(), 3);
    let original = payload(4 * 1024 * 1024);
    let mut shifted = original.clone();
    shifted.splice(8192..8192, b"inserted bytes".iter().copied());
    let first = cid::compute_cid(&original);
    let second = cid::compute_cid(&shifted);
    store.store(&first, &original).unwrap();
    let first_count = blobs(root.path()).len();
    store.store(&second, &shifted).unwrap();
    assert!(first_count > 8);
    assert!(blobs(root.path()).len() < first_count + 5);
    assert_eq!(store.read(&first).unwrap(), original);
    assert_eq!(store.read(&second).unwrap(), shifted);
    drop(store);
    let reopened = ContentStore::open(root.path()).unwrap();
    assert_eq!(reopened.read(&first).unwrap(), original);
    assert_eq!(
        reopened.chunking_profile(),
        Some(ChunkingProfile::default())
    );
}

#[test]
fn empty_file_and_corrupt_or_missing_chunks() {
    let root = TempDir::new().unwrap();
    let store = ContentStore::open(root.path()).unwrap();
    let empty = cid::compute_cid(b"");
    store.store(&empty, b"").unwrap();
    assert!(store.exists(&empty));
    assert!(store.read(&empty).unwrap().is_empty());
    let bytes = payload(1024 * 1024);
    let cid = cid::compute_cid(&bytes);
    store.store(&cid, &bytes).unwrap();
    let chunk = blobs(root.path())[0].clone();
    fs::write(&chunk, b"corrupt").unwrap();
    assert!(store.read(&cid).is_err());
    store.store(&cid, &bytes).unwrap();
    assert_eq!(store.read(&cid).unwrap(), bytes);
    fs::remove_file(chunk).unwrap();
    assert!(!store.exists(&cid));
    store.store(&cid, &bytes).unwrap();
    assert_eq!(store.read(&cid).unwrap(), bytes);
    assert!(store.store(&cid::compute_cid(b"wrong"), &bytes).is_err());
}

#[test]
fn persisted_profiles_cannot_be_changed_silently() {
    let root = TempDir::new().unwrap();
    drop(ContentStore::open(root.path()).unwrap());
    let other = ChunkingProfile {
        avg_size: 128 * 1024,
        ..Default::default()
    };
    assert!(ContentStore::open_with_profile(root.path(), other).is_err());
    let invalid = ChunkingProfile {
        min_size: 0,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());
    assert!(ChunkingProfile {
        version: 99,
        ..Default::default()
    }
    .validate()
    .is_err());
}

#[test]
fn migration_resumes_preserves_metadata_and_does_not_modify_source() {
    let source = TempDir::new().unwrap();
    fs::create_dir(source.path().join("empty")).unwrap();
    fs::create_dir(source.path().join("nested")).unwrap();
    fs::write(source.path().join("a.bin"), payload(2 * 1024 * 1024)).unwrap();
    fs::write(source.path().join("copy.bin"), payload(2 * 1024 * 1024)).unwrap();
    fs::write(source.path().join("nested/b.txt"), b"hello").unwrap();
    fs::write(source.path().join("zero"), b"").unwrap();
    let legacy = TempDir::new().unwrap();
    legacy_archive(legacy.path(), source.path());
    let original_db = fs::read(legacy.path().join("metadata.redb")).unwrap();
    let original_blobs = blobs(legacy.path())
        .into_iter()
        .map(|p| (p.clone(), fs::read(p).unwrap()))
        .collect::<Vec<_>>();
    let target = TempDir::new().unwrap();
    let processed = SyncCell::new(0);
    let result = migrate_serial(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        |p| processed.set(p.unique_files),
        || processed.get() >= 1,
    );
    assert!(result.is_err());
    assert!(Store::open(target.path()).is_err());
    let result = migrate_serial(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(result.unique_files, 3);
    assert_eq!(result.resumed_files, 1);
    assert_eq!(
        fs::read(legacy.path().join("metadata.redb")).unwrap(),
        original_db
    );
    for (path, bytes) in original_blobs {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    let old = Store::open_existing(legacy.path()).unwrap();
    let new = Store::open_existing(target.path()).unwrap();
    for path in [
        "/backup/a.bin",
        "/backup/copy.bin",
        "/backup/nested/b.txt",
        "/backup/zero",
    ] {
        let a = old.get_file(path).unwrap().unwrap();
        let b = new.get_file(path).unwrap().unwrap();
        assert_eq!(a.cid, b.cid);
        assert_eq!(
            (a.original_size, a.modified, a.created, a.permissions),
            (b.original_size, b.modified, b.created, b.permissions)
        );
        assert_eq!(old.read_file(path).unwrap(), new.read_file(path).unwrap());
    }
    assert_eq!(
        old.find_all_duplicates().unwrap(),
        new.find_all_duplicates().unwrap()
    );
    assert!(new
        .list_dir("/backup")
        .unwrap()
        .iter()
        .any(|e| e.name == "empty" && e.is_dir));
    drop(old);
    drop(new);
    assert_eq!(
        migrate_serial(
            legacy.path(),
            target.path(),
            ChunkingProfile::default(),
            |_| {},
            || false
        )
        .unwrap()
        .resumed_files,
        3
    );
}

#[test]
fn migration_rejects_unrelated_destinations_and_changed_sources() {
    let source = TempDir::new().unwrap();
    fs::write(source.path().join("a"), b"a").unwrap();
    fs::write(source.path().join("b"), b"b").unwrap();
    let legacy = TempDir::new().unwrap();
    legacy_archive(legacy.path(), source.path());
    let target = TempDir::new().unwrap();
    fs::write(target.path().join("keep"), b"untouched").unwrap();
    assert!(migrate_serial(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        |_| {},
        || false
    )
    .is_err());
    assert_eq!(fs::read(target.path().join("keep")).unwrap(), b"untouched");
    assert!(migrate_serial(
        legacy.path(),
        legacy.path(),
        ChunkingProfile::default(),
        |_| {},
        || false
    )
    .is_err());
    fs::remove_file(target.path().join("keep")).unwrap();
    assert!(migrate_serial(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        |_| {},
        || true
    )
    .is_err());
    fs::write(source.path().join("b"), b"changed content").unwrap();
    Store::open(legacy.path())
        .unwrap()
        .scan(source.path())
        .unwrap();
    assert!(migrate_serial(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        |_| {},
        || false
    )
    .is_err());
}

#[test]
fn statistics_count_shared_storage_once_across_files_and_extensions() {
    let source = TempDir::new().unwrap();
    let original = payload(2 * 1024 * 1024);
    let mut shifted = original.clone();
    shifted.splice(16384..16384, b"extra".iter().copied());
    fs::write(source.path().join("a.bin"), &original).unwrap();
    fs::write(source.path().join("copy.bin"), &original).unwrap();
    fs::write(source.path().join("b.txt"), &shifted).unwrap();
    let root = TempDir::new().unwrap();
    let store = Store::open(root.path()).unwrap();
    let scan = store.scan(source.path()).unwrap();
    let stats = store.compute_stats().unwrap();
    assert_eq!(stats.0, 3);
    assert_eq!(stats.2, 2);
    assert_eq!(scan.total_stored_bytes, stats.5);
    assert!(stats.5 < stats.4 / 2);
    let extensions = store.extension_stats().unwrap();
    assert_eq!(
        extensions.iter().map(|s| s.total_stored_bytes).sum::<u64>(),
        stats.5
    );
}

#[test]
fn migration_repairs_a_damaged_checkpoint_and_rejects_corrupt_source() {
    let source = TempDir::new().unwrap();
    fs::write(source.path().join("data"), payload(1024 * 1024)).unwrap();
    let legacy = TempDir::new().unwrap();
    legacy_archive(legacy.path(), source.path());
    let target = TempDir::new().unwrap();
    let processed = SyncCell::new(false);
    assert!(migrate_serial(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        |p| processed.set(p.unique_files > 0),
        || processed.get()
    )
    .is_err());
    fs::write(&blobs(target.path())[0], b"damaged checkpoint chunk").unwrap();
    assert_eq!(
        migrate_serial(
            legacy.path(),
            target.path(),
            ChunkingProfile::default(),
            |_| {},
            || false
        )
        .unwrap()
        .resumed_files,
        0
    );
    assert_eq!(
        Store::open(target.path())
            .unwrap()
            .read_file("/backup/data")
            .unwrap(),
        payload(1024 * 1024)
    );

    fs::write(&blobs(legacy.path())[0], b"damaged source").unwrap();
    let another = TempDir::new().unwrap();
    assert!(migrate_serial(
        legacy.path(),
        another.path(),
        ChunkingProfile::default(),
        |_| {},
        || false
    )
    .is_err());
    assert!(Store::open(another.path()).is_err());
}

#[test]
fn protected_destination_is_rejected_before_modifying_source() {
    let source = TempDir::new().unwrap();
    let legacy = TempDir::new().unwrap();
    legacy_archive(legacy.path(), source.path());
    let nested = legacy.path().join("blobs/nested/new-archive");
    assert!(migrate_serial(
        legacy.path(),
        &nested,
        ChunkingProfile::default(),
        |_| {},
        || false
    )
    .is_err());
    assert!(!legacy.path().join("blobs/nested").exists());
    for destination in [
        legacy.path().to_path_buf(),
        legacy.path().parent().unwrap().to_path_buf(),
        legacy.path().join("content.redb/new-archive"),
        legacy.path().join("metadata.redb/new-archive"),
    ] {
        assert!(migrate_serial(
            legacy.path(),
            &destination,
            ChunkingProfile::default(),
            |_| {},
            || false
        )
        .is_err());
    }
    assert!(!legacy.path().join("content.redb").exists());
}

#[cfg(unix)]
#[test]
fn migration_rejects_symlink_aliases_into_source_storage() {
    let source = TempDir::new().unwrap();
    let legacy = TempDir::new().unwrap();
    let aliases = TempDir::new().unwrap();
    legacy_archive(legacy.path(), source.path());
    let alias = aliases.path().join("alias");
    std::os::unix::fs::symlink(legacy.path().join("blobs"), &alias).unwrap();
    let result = migrate_serial(
        legacy.path(),
        &alias.join("fastcdc"),
        ChunkingProfile::default(),
        |_| {},
        || false,
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("source archive storage"));
    assert!(!legacy.path().join("blobs/fastcdc").exists());

    // The source's own blobs directory may also point outside its root.
    let external_blobs = aliases.path().join("external-blobs");
    fs::rename(legacy.path().join("blobs"), &external_blobs).unwrap();
    std::os::unix::fs::symlink(&external_blobs, legacy.path().join("blobs")).unwrap();
    let result = migrate_serial(
        legacy.path(),
        &external_blobs.join("fastcdc"),
        ChunkingProfile::default(),
        |_| {},
        || false,
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("source archive storage"));
    assert!(!external_blobs.join("fastcdc").exists());
}

#[test]
fn serial_and_parallel_scans_share_chunks_with_identical_results() {
    let source = TempDir::new().unwrap();
    let base = payload(1024 * 1024);
    for i in 0..75 {
        let mut version = base.clone();
        version.splice(16384..16384, format!("version {}", i % 5).bytes());
        fs::write(source.path().join(format!("{i}.bin")), version).unwrap();
    }
    let serial_dir = TempDir::new().unwrap();
    let parallel_dir = TempDir::new().unwrap();
    let serial = Store::open(serial_dir.path()).unwrap();
    let parallel = Store::open(parallel_dir.path()).unwrap();
    let scan = |store: &Store, workers| {
        store
            .scan_into_with_options(
                source.path(),
                "/",
                ScanOptions {
                    parallelism: Some(workers),
                    ..Default::default()
                },
                |_| {},
            )
            .unwrap()
    };
    let a = scan(&serial, 1);
    let b = scan(&parallel, 8);
    assert_eq!(
        (a.unique_blobs, a.duplicate_files, a.total_stored_bytes),
        (b.unique_blobs, b.duplicate_files, b.total_stored_bytes)
    );
    assert_eq!(
        serial.compute_stats().unwrap(),
        parallel.compute_stats().unwrap()
    );
    assert_eq!(a.total_stored_bytes, serial.compute_stats().unwrap().5);
    for i in 0..75 {
        let path = format!("/{i}.bin");
        assert_eq!(
            serial.read_file(&path).unwrap(),
            parallel.read_file(&path).unwrap()
        );
    }
}

fn parallel_fixture() -> (TempDir, TempDir, Vec<(String, Vec<u8>)>) {
    let input = TempDir::new().unwrap();
    let legacy = TempDir::new().unwrap();
    let base = payload(2 * 1024 * 1024);
    let mut files = Vec::new();
    for i in 0..8 {
        let mut bytes = base.clone();
        bytes.splice(16384..16384, format!("version-{i}").bytes());
        let name = format!("{i}.bin");
        fs::write(input.path().join(&name), &bytes).unwrap();
        files.push((name, bytes));
    }
    fs::write(input.path().join("copy.bin"), &files[0].1).unwrap();
    files.push(("copy.bin".into(), files[0].1.clone()));
    legacy_archive(legacy.path(), input.path());
    (input, legacy, files)
}

#[test]
fn parallel_migration_matches_serial_and_updates_inside_files() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let (_input, legacy, files) = parallel_fixture();
    let serial = TempDir::new().unwrap();
    let parallel = match std::env::var_os("DEDUP_TEST_FS_ROOT") {
        Some(root) => TempDir::new_in(root).unwrap(),
        None => TempDir::new().unwrap(),
    };
    migrate_serial(
        legacy.path(),
        serial.path(),
        ChunkingProfile::default(),
        |_| {},
        || false,
    )
    .unwrap();
    let max_active = AtomicUsize::new(0);
    let saw_partial = AtomicBool::new(false);
    let threads = Mutex::new(std::collections::HashSet::new());
    let progress = migration::migrate_with_options(
        legacy.path(),
        parallel.path(),
        ChunkingProfile::default(),
        migration::MigrationOptions { workers: 4 },
        |p| {
            max_active.fetch_max(p.active_files.len(), Ordering::Relaxed);
            if p.unique_files == 0 && p.work_bytes > 0 {
                saw_partial.store(true, Ordering::Relaxed);
            }
            let thread = std::thread::current();
            if thread
                .name()
                .is_some_and(|name| name.starts_with("migration-") && name != "migration-progress")
            {
                threads.lock().unwrap().insert(thread.id());
            }
        },
        || false,
    )
    .unwrap();
    assert!(max_active.load(Ordering::Relaxed) >= 2);
    assert!(threads.lock().unwrap().len() >= 2);
    assert!(saw_partial.load(Ordering::Relaxed));
    assert_eq!(progress.unique_files, 8);
    assert_eq!(progress.workers, 4);
    assert_eq!(progress.phase, migration::MigrationPhase::Completed);
    assert_eq!(progress.work_bytes, 2 * progress.total_bytes);
    let target = Store::open_existing(parallel.path()).unwrap();
    for (name, bytes) in files {
        assert_eq!(target.read_file(&format!("/backup/{name}")).unwrap(), bytes);
    }
    let serial_store = Store::open_existing(serial.path()).unwrap();
    assert_eq!(
        target.compute_stats().unwrap(),
        serial_store.compute_stats().unwrap()
    );
    for name in (0..8)
        .map(|i| format!("/backup/{i}.bin"))
        .chain(["/backup/copy.bin".into()])
    {
        assert_eq!(
            bincode::serialize(&target.metadata.get_file(&name).unwrap()).unwrap(),
            bincode::serialize(&serial_store.metadata.get_file(&name).unwrap()).unwrap()
        );
    }
    assert_eq!(
        bincode::serialize(&target.metadata.list_dir("/backup").unwrap()).unwrap(),
        bincode::serialize(&serial_store.metadata.list_dir("/backup").unwrap()).unwrap()
    );
    drop(target);
    let resumed = migration::migrate_with_options(
        legacy.path(),
        parallel.path(),
        ChunkingProfile::default(),
        migration::MigrationOptions { workers: 2 },
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(resumed.resumed_files, 8);
    assert_eq!(resumed.work_bytes, resumed.total_bytes);
}

#[test]
fn parallel_cancellation_resumes_with_a_different_worker_count() {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    let (_input, legacy, files) = parallel_fixture();
    let target = TempDir::new().unwrap();
    let cancel = AtomicBool::new(false);
    let completed = AtomicU64::new(0);
    let result = migration::migrate_with_options(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        migration::MigrationOptions { workers: 4 },
        |p| {
            completed.store(p.unique_files, Ordering::Relaxed);
            if p.unique_files >= 2 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        || cancel.load(Ordering::Relaxed),
    );
    assert!(format!("{:#}", result.unwrap_err()).contains("migration cancelled"));
    assert!(Store::open_existing(target.path()).is_err());
    let resumed = migration::migrate_with_options(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        migration::MigrationOptions { workers: 2 },
        |_| {},
        || false,
    )
    .unwrap();
    assert!(resumed.resumed_files >= completed.load(Ordering::Relaxed));
    assert_eq!(resumed.unique_files, 8);
    let target = Store::open_existing(target.path()).unwrap();
    for (name, bytes) in files {
        assert_eq!(target.read_file(&format!("/backup/{name}")).unwrap(), bytes);
    }
}

#[test]
fn parallel_source_failure_keeps_the_destination_incomplete() {
    let (_input, legacy, files) = parallel_fixture();
    let target = TempDir::new().unwrap();
    let cid = cid::compute_cid(&files[0].1);
    fs::write(
        legacy.path().join("blobs").join(format!("{cid}.lz4")),
        b"corrupt source",
    )
    .unwrap();
    let error = migration::migrate_with_options(
        legacy.path(),
        target.path(),
        ChunkingProfile::default(),
        migration::MigrationOptions { workers: 4 },
        |_| {},
        || false,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains(&format!("failed to migrate CID {cid}")));
    assert!(Store::open_existing(target.path()).is_err());
}

#[test]
fn heartbeat_reports_waiting_on_disk_before_a_file_completes() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    };
    let input = TempDir::new().unwrap();
    let legacy = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let data = payload(1024 * 1024);
    let first_chunk = fastcdc::v2020::FastCDC::with_level(
        &data,
        16384,
        65536,
        262144,
        fastcdc::v2020::Normalization::Level1,
    )
    .next()
    .unwrap();
    let first_cid =
        cid::compute_cid(&data[first_chunk.offset..first_chunk.offset + first_chunk.length]);
    let digest = first_cid.hash().digest();
    let shard = format!("blobs/{:02x}/{:x}", digest[0], digest[1] >> 4);
    fs::write(input.path().join("large"), &data).unwrap();
    legacy_archive(legacy.path(), input.path());
    let held_lock = Arc::new(Mutex::new(None));
    let acquire = AtomicBool::new(true);
    let (tx, rx) = mpsc::channel();
    let source_path = legacy.path().to_path_buf();
    let destination = target.path().to_path_buf();
    let held_by_worker = Arc::clone(&held_lock);
    let handle = std::thread::spawn(move || {
        migration::migrate_with_options(
            &source_path,
            &destination,
            ChunkingProfile::default(),
            migration::MigrationOptions { workers: 1 },
            |p| {
                if p.work_bytes > 0 && acquire.swap(false, Ordering::Relaxed) {
                    fs::create_dir_all(destination.join(&shard)).unwrap();
                    let file = fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(false)
                        .open(destination.join(&shard).join(".publish.lock"))
                        .unwrap();
                    file.lock().unwrap();
                    *held_by_worker.lock().unwrap() = Some(file);
                }
                tx.send(p.clone()).unwrap();
            },
            || false,
        )
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut saw_heartbeat = false;
    while std::time::Instant::now() < deadline {
        if let Ok(p) = rx.recv_timeout(std::time::Duration::from_millis(700)) {
            if p.unique_files == 0 && p.work_bytes > 0 && p.idle_seconds >= 0.4 {
                assert!(!p.active_files.is_empty());
                saw_heartbeat = true;
                break;
            }
        }
    }
    drop(held_lock.lock().unwrap().take());
    handle.join().unwrap().unwrap();
    assert!(
        saw_heartbeat,
        "no heartbeat while publication waited for the disk lock"
    );
}

#[test]
fn explicit_format_survives_reopening_before_and_after_scans() {
    use dedup_core::StorageFormat;
    for (format, version) in [(StorageFormat::Legacy, 1), (StorageFormat::Fastcdc, 3)] {
        let tmp = TempDir::new().unwrap();
        let archive = tmp.path().join("archive");
        let input = tmp.path().join("input");
        fs::create_dir(&input).unwrap();
        fs::write(input.join("a"), b"shared content").unwrap();
        fs::write(input.join("copy"), b"shared content").unwrap();
        let store = Store::create(&archive, format).unwrap();
        assert_eq!(store.content.format_version(), version);
        drop(store);
        let store = Store::open_existing(&archive).unwrap();
        assert_eq!(store.content.format_version(), version);
        let result = store.scan_into(&input, "/", |_| {}).unwrap();
        assert_eq!(result.duplicate_files, 1);
        drop(store);
        let store = Store::open_existing(&archive).unwrap();
        assert_eq!(store.content.format_version(), version);
        assert_eq!(store.read_file("/a").unwrap(), b"shared content");
        assert_eq!(store.read_file("/copy").unwrap(), b"shared content");
        fs::write(input.join("new"), b"another file").unwrap();
        store.scan_into(&input, "/", |_| {}).unwrap();
        assert_eq!(store.read_file("/new").unwrap(), b"another file");
        assert_eq!(archive.join("content.redb").exists(), version >= 2);
    }
}

#[test]
fn explicit_creation_rejects_nonempty_locations_and_existing_archives() {
    use dedup_core::StorageFormat;
    let tmp = TempDir::new().unwrap();
    let data = tmp.path().join("personal");
    fs::create_dir(&data).unwrap();
    fs::write(data.join("keep"), b"unchanged").unwrap();
    for format in [StorageFormat::Legacy, StorageFormat::Fastcdc] {
        assert!(Store::create(&data, format)
            .err()
            .unwrap()
            .to_string()
            .contains("empty"));
        assert_eq!(fs::read(data.join("keep")).unwrap(), b"unchanged");
        assert_eq!(fs::read_dir(&data).unwrap().count(), 1);
    }
    for format in [StorageFormat::Legacy, StorageFormat::Fastcdc] {
        let archive = tmp.path().join(format!("{format:?}"));
        // Selecting an already-existing empty directory is supported too.
        fs::create_dir(&archive).unwrap();
        drop(Store::create(&archive, format).unwrap());
        let metadata = fs::read(archive.join("metadata.redb")).unwrap();
        for requested in [StorageFormat::Legacy, StorageFormat::Fastcdc] {
            assert!(Store::create(&archive, requested).is_err());
            assert_eq!(fs::read(archive.join("metadata.redb")).unwrap(), metadata);
        }
        let store = Store::open_existing(&archive).unwrap();
        assert_eq!(
            store.content.format_version(),
            if format == StorageFormat::Legacy {
                1
            } else {
                3
            }
        );
    }
}
