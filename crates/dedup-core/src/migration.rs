//! Resumable, verified legacy-to-FastCDC migration into a separate directory.
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Condvar, Mutex,
};
use std::time::{Duration, Instant};

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

use crate::content_store::{lock_file, sync_dir};
use crate::{cid, ChunkingProfile, ContentStore, FileMetadata, MetadataDb, Store};

const STATE_FILE: &str = "migration.bin";
const LOCK_FILE: &str = ".migration.lock";

#[derive(Debug, Clone, Copy)]
pub struct MigrationOptions {
    pub workers: usize,
}
impl Default for MigrationOptions {
    fn default() -> Self {
        Self {
            workers: std::thread::available_parallelism().map_or(1, |n| n.get().min(4)),
        }
    }
}
impl MigrationOptions {
    pub fn validate(self) -> Result<()> {
        ensure!(
            (1..=32).contains(&self.workers),
            "migration workers must be between 1 and 32"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationPhase {
    #[default]
    Preparing,
    Migrating,
    Finalizing,
    Completed,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationFileStage {
    Converting,
    Verifying,
    SavingCheckpoint,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationFileProgress {
    pub path: String,
    pub stage: MigrationFileStage,
    pub bytes_processed: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MigrationProgress {
    pub unique_files: u64,
    pub resumed_files: u64,
    pub total_unique_files: u64,
    pub bytes_processed: u64,
    pub total_bytes: u64,
    /// Uncompressed bytes read for conversion and destination verification.
    /// Re-reading a checkpoint contributes work again; this is not disk write speed.
    pub work_bytes: u64,
    pub elapsed_seconds: f64,
    pub rate_bytes_per_second: f64,
    pub idle_seconds: f64,
    pub workers: usize,
    pub phase: MigrationPhase,
    pub active_files: Vec<MigrationFileProgress>,
}

#[derive(Serialize, Deserialize)]
struct MigrationState {
    version: u32,
    source: String,
    snapshot_hash: [u8; 32],
    profile: ChunkingProfile,
    complete: bool,
}

pub(crate) fn ensure_complete(root: &Path) -> Result<()> {
    if root.join(STATE_FILE).exists() {
        let state = read_state(root)?;
        ensure!(
            state.complete,
            "migration is incomplete; resume with dedup migrate --source {} --destination {}",
            state.source,
            root.display()
        );
    }
    Ok(())
}

fn read_state(root: &Path) -> Result<MigrationState> {
    let state: MigrationState = bincode::deserialize(&fs::read(root.join(STATE_FILE))?)
        .context("invalid migration checkpoint")?;
    ensure!(
        state.version == 1,
        "unsupported migration checkpoint version"
    );
    Ok(state)
}

fn write_state(root: &Path, state: &MigrationState, initial: bool) -> Result<()> {
    let target = root.join(STATE_FILE);
    let tmp = root.join("migration.bin.tmp");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)?;
    file.write_all(&bincode::serialize(state)?)?;
    file.sync_all()?;
    drop(file);
    if initial {
        ensure!(!target.exists(), "migration checkpoint already exists");
    }
    fs::rename(&tmp, &target).context("failed to publish migration checkpoint")?;
    sync_dir(root)
}

/// Resolve missing descendants through their closest existing ancestor, so a
/// protected destinations are rejected before creating anything inside the source.
fn resolve_destination(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(path.canonicalize()?);
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut ancestor = absolute.as_path();
    let mut missing = Vec::new();
    while !ancestor.exists() {
        missing.push(
            ancestor
                .file_name()
                .context("invalid destination path")?
                .to_os_string(),
        );
        ancestor = ancestor
            .parent()
            .context("destination has no existing ancestor")?;
    }
    let mut resolved = ancestor.canonicalize()?;
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

/// Convert every distinct legacy file CID once, preserving metadata and exact
/// duplicate groups. Re-running the same command resumes durable manifests and
/// metadata checkpoints. The source is opened without writing any transactions.
/// A changed source snapshot or unrelated/non-empty destination is rejected.
pub fn migrate<F, C>(
    source: &Path,
    destination: &Path,
    profile: ChunkingProfile,
    on_progress: F,
    should_cancel: C,
) -> Result<MigrationProgress>
where
    F: Fn(&MigrationProgress) + Sync,
    C: Fn() -> bool + Sync,
{
    migrate_with_options(
        source,
        destination,
        profile,
        MigrationOptions::default(),
        on_progress,
        should_cancel,
    )
}

pub fn migrate_with_options<F, C>(
    source: &Path,
    destination: &Path,
    profile: ChunkingProfile,
    options: MigrationOptions,
    on_progress: F,
    should_cancel: C,
) -> Result<MigrationProgress>
where
    F: Fn(&MigrationProgress) + Sync,
    C: Fn() -> bool + Sync,
{
    let source = source
        .canonicalize()
        .context("source store does not exist")?;
    let source_store = Store::open_existing(&source)?;
    migrate_store_with_options(
        &source_store,
        &source,
        destination,
        profile,
        options,
        on_progress,
        should_cancel,
    )
}

/// Reuse an already-open archive while redb holds its exclusive database lock.
pub fn migrate_store<F, C>(
    source_store: &Store,
    source: &Path,
    destination: &Path,
    profile: ChunkingProfile,
    on_progress: F,
    should_cancel: C,
) -> Result<MigrationProgress>
where
    F: Fn(&MigrationProgress) + Sync,
    C: Fn() -> bool + Sync,
{
    migrate_store_with_options(
        source_store,
        source,
        destination,
        profile,
        MigrationOptions::default(),
        on_progress,
        should_cancel,
    )
}

/// Parallelism is a runtime option, not part of the persisted chunking profile;
/// it can change on resume. Callbacks are serialized and may run on worker threads.
pub fn migrate_store_with_options<F, C>(
    source_store: &Store,
    source: &Path,
    destination: &Path,
    profile: ChunkingProfile,
    options: MigrationOptions,
    on_progress: F,
    should_cancel: C,
) -> Result<MigrationProgress>
where
    F: Fn(&MigrationProgress) + Sync,
    C: Fn() -> bool + Sync,
{
    options.validate()?;
    profile.validate()?;
    let source = source
        .canonicalize()
        .context("source store does not exist")?;
    ensure!(
        source_store.root.canonicalize()? == source,
        "source handle belongs to another archive"
    );
    ensure!(
        source_store.content.format_version() == 1,
        "source already uses FastCDC; migration requires a legacy store"
    );
    let snapshot = source_store.metadata.snapshot()?;
    let snapshot_hash = *blake3::hash(&bincode::serialize(&snapshot)?).as_bytes();

    // Read from the metadata snapshot, not a disk walk: a dedicated child
    // directory is safe even when the legacy archive occupies a volume root.
    // Resolve aliases before writing anything and protect the source's storage.
    let destination = resolve_destination(destination)?;
    ensure!(
        !source.starts_with(&destination),
        "destination must differ from the source archive and cannot contain it"
    );
    for name in [
        "blobs",
        "metadata.redb",
        "content.redb",
        STATE_FILE,
        "migration.bin.tmp",
        LOCK_FILE,
    ] {
        let owned = source.join(name);
        ensure!(
            !destination.starts_with(&owned),
            "destination cannot be inside source archive storage: {}",
            owned.display()
        );
        if owned.exists() {
            let resolved = owned
                .canonicalize()
                .context("cannot resolve source archive storage")?;
            ensure!(
                !destination.starts_with(&resolved),
                "destination cannot be inside source archive storage: {}",
                owned.display()
            );
        }
    }
    fs::create_dir_all(&destination)?;
    if let Some(parent) = destination.parent() {
        sync_dir(parent)?;
    }
    // Serializes initial marker creation and conversion, even for different
    // source archives targeting the same directory. Drop releases the OS lock.
    let _migration_lock = lock_file(&destination.join(LOCK_FILE))?;
    let mut state = if destination.join(STATE_FILE).exists() {
        let state = read_state(&destination)?;
        ensure!(
            state.source == source.to_string_lossy() && state.snapshot_hash == snapshot_hash,
            "source archive changed or destination belongs to another migration"
        );
        ensure!(
            state.profile == profile,
            "migration chunking profile differs from checkpoint"
        );
        state
    } else {
        ensure!(
            fs::read_dir(&destination)?.all(|entry| entry
                .is_ok_and(|e| e.file_name() == "migration.bin.tmp" || e.file_name() == LOCK_FILE)),
            "migration destination must be empty"
        );
        let state = MigrationState {
            version: 1,
            source: source.to_string_lossy().into_owned(),
            snapshot_hash,
            profile,
            complete: false,
        };
        write_state(&destination, &state, true)?;
        state
    };

    let target = Store {
        content: ContentStore::open_with_profile(&destination, profile)?,
        metadata: MetadataDb::open(&destination.join("metadata.redb"))?,
        root: destination.clone(),
    };
    let mut groups = BTreeMap::new();
    for (path, meta) in snapshot.files {
        let file_cid = cid::cid_from_bytes(&meta.cid)?;
        groups
            .entry(cid::cid_to_string(&file_cid))
            .or_insert_with(Vec::new)
            .push((path, meta));
    }
    let total_bytes = groups.values().try_fold(0u64, |sum, files| {
        sum.checked_add(files[0].1.original_size)
            .context("archive size overflow")
    })?;
    let workers = options.workers.min(groups.len().max(1));
    let tracker = ProgressTracker::new(
        MigrationProgress {
            total_unique_files: groups.len() as u64,
            total_bytes,
            workers,
            phase: MigrationPhase::Migrating,
            ..Default::default()
        },
        &on_progress,
    );
    tracker.emit(true);
    let queue = Mutex::new(groups.into_iter());
    let failure = Mutex::new(None);
    let abort = AtomicBool::new(false);
    let cancelled = || abort.load(Ordering::Relaxed) || should_cancel();
    let stop = (Mutex::new(false), Condvar::new());
    std::thread::scope(|scope| -> Result<()> {
        let _stop = StopHeartbeat(&stop);
        let heartbeat = std::thread::Builder::new()
            .name("migration-progress".into())
            .spawn_scoped(scope, || loop {
                let guard = stop.0.lock().expect("heartbeat lock poisoned");
                let (guard, _) = stop
                    .1
                    .wait_timeout(guard, Duration::from_millis(500))
                    .expect("heartbeat lock poisoned");
                if *guard {
                    break;
                }
                drop(guard);
                tracker.emit(true);
            })?;
        let mut handles = Vec::new();
        for worker in 0..workers {
            let (queue, failure, abort, tracker, target, cancelled) =
                (&queue, &failure, &abort, &tracker, &target, &cancelled);
            let handle = std::thread::Builder::new()
                .name(format!("migration-{worker}"))
                .spawn_scoped(scope, move || loop {
                    if cancelled() {
                        break;
                    }
                    let job = queue.lock().expect("migration queue poisoned").next();
                    let Some((cid_string, files)) = job else {
                        break;
                    };
                    if let Err(error) = migrate_file(
                        source_store,
                        target,
                        cid_string,
                        files,
                        worker,
                        tracker,
                        cancelled,
                    ) {
                        let mut first = failure.lock().expect("migration error lock poisoned");
                        if first.is_none() {
                            *first = Some(error);
                        }
                        abort.store(true, Ordering::Relaxed);
                        break;
                    }
                });
            match handle {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    *failure.lock().expect("migration error lock poisoned") = Some(error.into());
                    abort.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }
        for handle in handles {
            if handle.join().is_err() {
                let mut first = failure.lock().expect("migration error lock poisoned");
                if first.is_none() {
                    *first = Some(anyhow::anyhow!("migration worker panicked"));
                }
                abort.store(true, Ordering::Relaxed);
            }
        }
        if let Some(error) = failure
            .lock()
            .expect("migration error lock poisoned")
            .take()
        {
            return Err(error);
        }
        ensure!(
            !should_cancel(),
            "migration cancelled; run the same command to resume"
        );
        tracker.phase(MigrationPhase::Finalizing);
        target.metadata.write_batch(&[], &snapshot.dirs, true)?;
        state.complete = true;
        write_state(&destination, &state, false)?;
        tracker.phase(MigrationPhase::Completed);
        drop(_stop);
        heartbeat
            .join()
            .map_err(|_| anyhow::anyhow!("migration progress thread panicked"))?;
        Ok(())
    })?;
    Ok(tracker.snapshot())
}

struct StopHeartbeat<'a>(&'a (Mutex<bool>, Condvar));
impl Drop for StopHeartbeat<'_> {
    fn drop(&mut self) {
        *self.0 .0.lock().expect("heartbeat lock poisoned") = true;
        self.0 .1.notify_all();
    }
}

struct ProgressState {
    progress: MigrationProgress,
    active: BTreeMap<usize, MigrationFileProgress>,
    last_emit: Instant,
    last_activity: Instant,
    last_rate: Instant,
    last_work_bytes: u64,
}
struct ProgressTracker<'a, F> {
    state: Mutex<ProgressState>,
    started: Instant,
    callback: &'a F,
}
impl<F: Fn(&MigrationProgress)> ProgressTracker<'_, F> {
    fn new(progress: MigrationProgress, callback: &F) -> ProgressTracker<'_, F> {
        let now = Instant::now();
        ProgressTracker {
            state: Mutex::new(ProgressState {
                progress,
                active: BTreeMap::new(),
                last_emit: now,
                last_activity: now,
                last_rate: now,
                last_work_bytes: 0,
            }),
            started: now,
            callback,
        }
    }
    fn refresh(&self, state: &mut ProgressState) {
        let now = Instant::now();
        state.progress.elapsed_seconds = now.duration_since(self.started).as_secs_f64();
        state.progress.idle_seconds = now.duration_since(state.last_activity).as_secs_f64();
        let elapsed = now.duration_since(state.last_rate).as_secs_f64();
        if elapsed >= 0.25 {
            state.progress.rate_bytes_per_second =
                (state.progress.work_bytes - state.last_work_bytes) as f64 / elapsed;
            state.last_work_bytes = state.progress.work_bytes;
            state.last_rate = now;
        }
        state.progress.active_files = state.active.values().cloned().collect();
    }
    fn publish(&self, state: &mut ProgressState, force: bool) {
        if force || state.last_emit.elapsed() >= Duration::from_millis(250) {
            self.refresh(state);
            state.last_emit = Instant::now();
            (self.callback)(&state.progress);
        }
    }
    fn emit(&self, force: bool) {
        self.publish(
            &mut self.state.lock().expect("migration progress poisoned"),
            force,
        );
    }
    fn snapshot(&self) -> MigrationProgress {
        let mut state = self.state.lock().expect("migration progress poisoned");
        self.refresh(&mut state);
        state.progress.clone()
    }
    fn phase(&self, phase: MigrationPhase) {
        let mut state = self.state.lock().expect("migration progress poisoned");
        state.progress.phase = phase;
        state.last_activity = Instant::now();
        self.publish(&mut state, true);
    }
    fn stage(&self, worker: usize, path: &str, stage: MigrationFileStage, total_bytes: u64) {
        let mut state = self.state.lock().expect("migration progress poisoned");
        state.active.insert(
            worker,
            MigrationFileProgress {
                path: path.into(),
                stage,
                bytes_processed: if stage == MigrationFileStage::SavingCheckpoint {
                    total_bytes
                } else {
                    0
                },
                total_bytes,
            },
        );
        state.last_activity = Instant::now();
        self.publish(&mut state, false);
    }
    fn bytes(&self, worker: usize, bytes: usize) {
        if bytes == 0 {
            return;
        }
        let mut state = self.state.lock().expect("migration progress poisoned");
        let first_bytes = state.progress.work_bytes == 0;
        state.progress.work_bytes = state.progress.work_bytes.saturating_add(bytes as u64);
        if let Some(file) = state.active.get_mut(&worker) {
            file.bytes_processed = file
                .bytes_processed
                .saturating_add(bytes as u64)
                .min(file.total_bytes);
        }
        state.last_activity = Instant::now();
        self.publish(&mut state, first_bytes);
    }
    fn complete(&self, worker: usize, bytes: u64, resumed: bool) {
        let mut state = self.state.lock().expect("migration progress poisoned");
        state.active.remove(&worker);
        state.progress.unique_files += 1;
        state.progress.bytes_processed += bytes;
        state.progress.resumed_files += u64::from(resumed);
        state.last_activity = Instant::now();
        self.publish(&mut state, true);
    }
}

fn migrate_file<F, C>(
    source: &Store,
    target: &Store,
    cid_string: String,
    files: Vec<(String, FileMetadata)>,
    worker: usize,
    tracker: &ProgressTracker<'_, F>,
    should_cancel: &C,
) -> Result<()>
where
    F: Fn(&MigrationProgress),
    C: Fn() -> bool,
{
    ensure!(
        !should_cancel(),
        "migration cancelled; run the same command to resume"
    );
    let file_cid = cid::cid_from_string(&cid_string)?;
    let expected_size = files[0].1.original_size;
    let path = &files[0].0;
    ensure!(
        files
            .iter()
            .all(|(_, meta)| meta.original_size == expected_size),
        "inconsistent source sizes for CID {file_cid}"
    );
    let bytes = |n| tracker.bytes(worker, n);
    let verified_size = if target.content.exists(&file_cid) {
        tracker.stage(worker, path, MigrationFileStage::Verifying, expected_size);
        target
            .content
            .copy_to(
                &file_cid,
                ProgressSink {
                    should_cancel,
                    on_bytes: &bytes,
                },
            )
            .ok()
    } else {
        None
    };
    let resumed = verified_size == Some(expected_size);
    if !resumed {
        ensure!(
            !should_cancel(),
            "migration cancelled; run the same command to resume"
        );
        tracker.stage(worker, path, MigrationFileStage::Converting, expected_size);
        target
            .content
            .store_reader(
                &file_cid,
                ProgressReader {
                    inner: source.content.migration_reader(&file_cid)?,
                    should_cancel,
                    on_bytes: &bytes,
                },
            )
            .with_context(|| format!("failed to migrate CID {file_cid}"))?;
        tracker.stage(worker, path, MigrationFileStage::Verifying, expected_size);
        let actual_size = target.content.copy_to(
            &file_cid,
            ProgressSink {
                should_cancel,
                on_bytes: &bytes,
            },
        )?;
        ensure!(
            actual_size == expected_size,
            "migrated file length mismatch: {file_cid}"
        );
    }
    ensure!(
        !should_cancel(),
        "migration cancelled; run the same command to resume"
    );
    tracker.stage(
        worker,
        path,
        MigrationFileStage::SavingCheckpoint,
        expected_size,
    );
    let stored_size = target
        .content
        .compressed_size(&file_cid)?
        .context("missing migrated content")?;
    let files = files
        .into_iter()
        .map(|(path, mut meta)| {
            meta.compressed_size = stored_size;
            (path, meta, cid_string.clone())
        })
        .collect::<Vec<_>>();
    target.metadata.write_batch(&files, &[], true)?;
    tracker.complete(worker, expected_size, resumed);
    Ok(())
}

struct ProgressReader<'a, R, C, B> {
    inner: R,
    should_cancel: &'a C,
    on_bytes: &'a B,
}
impl<R: std::io::Read, C: Fn() -> bool, B: Fn(usize)> std::io::Read
    for ProgressReader<'_, R, C, B>
{
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if (self.should_cancel)() {
            return Err(std::io::Error::other("migration cancelled"));
        }
        let n = self.inner.read(buffer)?;
        (self.on_bytes)(n);
        Ok(n)
    }
}
struct ProgressSink<'a, C, B> {
    should_cancel: &'a C,
    on_bytes: &'a B,
}
impl<C: Fn() -> bool, B: Fn(usize)> Write for ProgressSink<'_, C, B> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if (self.should_cancel)() {
            return Err(std::io::Error::other("migration cancelled"));
        }
        (self.on_bytes)(buffer.len());
        Ok(buffer.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn resume_after_interrupted_initial_format_transaction() {
        let source = TempDir::new().unwrap();
        fs::create_dir(source.path().join("blobs")).unwrap();
        let store = Store::open(source.path()).unwrap();
        let snapshot_hash =
            *blake3::hash(&bincode::serialize(&store.metadata.snapshot().unwrap()).unwrap())
                .as_bytes();
        drop(store);
        let target = TempDir::new().unwrap();
        write_state(
            target.path(),
            &MigrationState {
                version: 1,
                source: source
                    .path()
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                snapshot_hash,
                profile: ChunkingProfile::default(),
                complete: false,
            },
            true,
        )
        .unwrap();
        drop(redb::Database::create(target.path().join("content.redb")).unwrap());
        migrate(
            source.path(),
            target.path(),
            ChunkingProfile::default(),
            |_| {},
            || false,
        )
        .unwrap();
        assert_eq!(
            Store::open(target.path()).unwrap().content.format_version(),
            2
        );
    }
}
