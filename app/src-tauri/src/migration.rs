use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use dedup_core::{
    migration::{self, MigrationOptions, MigrationPhase, MigrationProgress},
    ChunkingProfile, Store,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::AppState;
use crate::workspace::{self, PendingMigration, Workspace, WorkspaceStats};

pub(crate) struct OperationGuard(Arc<AtomicBool>);
impl OperationGuard {
    pub(crate) fn acquire(state: &AppState) -> Result<Self, String> {
        state
            .operation_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Wait for the current scan or migration to finish.".to_string())?;
        Ok(Self(Arc::clone(&state.operation_running)))
    }
}
impl Drop for OperationGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

struct MigrationJob {
    state: AppState,
    _operation: OperationGuard,
}
impl MigrationJob {
    fn start(state: &AppState, job_id: &str) -> Result<Self, String> {
        if job_id.trim().is_empty() {
            return Err("Missing migration job ID.".into());
        }
        let operation = OperationGuard::acquire(state)?;
        let mut job = state.migration_job.lock().map_err(|e| e.to_string())?;
        state.migration_cancelled.store(false, Ordering::Relaxed);
        *job = Some(job_id.to_owned());
        Ok(Self {
            state: state.clone(),
            _operation: operation,
        })
    }
}
impl Drop for MigrationJob {
    fn drop(&mut self) {
        if let Ok(mut job) = self.state.migration_job.lock() {
            *job = None;
        }
    }
}

#[derive(Clone, Serialize)]
struct ProgressEvent {
    job_id: String,
    progress: MigrationProgress,
}

fn source_workspace(state: &AppState, id: &str) -> Result<Workspace, String> {
    state
        .workspaces
        .lock()
        .map_err(|e| e.to_string())?
        .find(id)
        .cloned()
        .ok_or_else(|| "Archive no longer exists in the list.".to_string())
}

/// Reuse the active store so the source DB is never opened twice. Keep its
/// mutex while migrating; cancellation and progress do not depend on this lock.
fn with_source<T>(
    state: &AppState,
    source: &Path,
    operation: impl FnOnce(&Store) -> Result<T, String>,
) -> Result<T, String> {
    let source = source
        .canonicalize()
        .map_err(|e| format!("Cannot open source archive: {e}"))?;
    let mut guard = state.store.lock().map_err(|e| e.to_string())?;
    let active = state
        .store_path
        .lock()
        .map_err(|e| e.to_string())?
        .canonicalize()
        .ok();
    if active.as_ref() == Some(&source) {
        if guard.is_none() {
            *guard = Some(Store::open_existing(&source).map_err(|e| format!("{e:#}"))?);
        }
        operation(guard.as_ref().unwrap())
    } else {
        drop(guard);
        let store = Store::open_existing(&source).map_err(|e| format!("{e:#}"))?;
        operation(&store)
    }
}

#[tauri::command]
pub async fn workspace_storage_format(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<u32, String> {
    state
        .run_blocking(move |state| {
            let _operation = OperationGuard::acquire(state)?;
            let source = source_workspace(state, &workspace_id)?;
            with_source(state, Path::new(&source.store_path), |store| {
                Ok(store.content.format_version())
            })
        })
        .await
}

#[tauri::command]
pub fn cancel_migration(state: State<'_, AppState>, job_id: String) -> Result<(), String> {
    request_cancel(&state, &job_id)
}
fn request_cancel(state: &AppState, job_id: &str) -> Result<(), String> {
    let job = state.migration_job.lock().map_err(|e| e.to_string())?;
    if job.as_deref() == Some(job_id) {
        state.migration_cancelled.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn migrate_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
    destination: String,
    label: String,
    job_id: String,
    workers: Option<usize>,
) -> Result<Workspace, String> {
    let workers = workers.unwrap_or_else(|| MigrationOptions::default().workers);
    MigrationOptions { workers }
        .validate()
        .map_err(|e| e.to_string())?;
    let job = MigrationJob::start(&state, &job_id)?;
    state
        .run_blocking(move |state| {
            let _job = job;
            with_progress_events(
                workers,
                |progress| {
                    let _ = app.emit(
                        "migration-progress",
                        ProgressEvent {
                            job_id: job_id.clone(),
                            progress,
                        },
                    );
                },
                |on_progress| {
                    migrate_archive(
                        state,
                        &workspace_id,
                        &destination,
                        &label,
                        workers,
                        on_progress,
                    )
                },
            )
        })
        .await
}

struct StopProgress<'a> {
    done: &'a AtomicBool,
    thread: std::thread::Thread,
}
impl Drop for StopProgress<'_> {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        self.thread.unpark();
    }
}

/// Keep updates flowing while reading the index, migrating, calculating archive
/// totals and registering the result. A heartbeat reports responsiveness; the
/// separate idle time reports actual data activity, rather than implying progress.
fn with_progress_events<T>(
    workers: usize,
    emit: impl Fn(MigrationProgress) + Sync,
    operation: impl FnOnce(&(dyn Fn(&MigrationProgress) + Sync)) -> Result<T, String>,
) -> Result<T, String> {
    let started = std::time::Instant::now();
    let cached = std::sync::Mutex::new((
        MigrationProgress {
            workers,
            ..Default::default()
        },
        started,
    ));
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("migration-ui-progress".into())
            .spawn_scoped(scope, || {
                while !done.load(Ordering::Acquire) {
                    let (mut progress, updated) = cached
                        .lock()
                        .expect("migration progress cache poisoned")
                        .clone();
                    progress.elapsed_seconds = started.elapsed().as_secs_f64();
                    progress.idle_seconds += updated.elapsed().as_secs_f64();
                    if updated.elapsed().as_secs_f64() > 1.0 {
                        progress.rate_bytes_per_second = 0.0;
                    }
                    if progress.phase == MigrationPhase::Completed {
                        progress.phase = MigrationPhase::Finalizing;
                    }
                    emit(progress);
                    std::thread::park_timeout(std::time::Duration::from_millis(500));
                }
            })
            .map_err(|e| e.to_string())?;
        let stop = StopProgress {
            done: &done,
            thread: handle.thread().clone(),
        };
        let callback = |progress: &MigrationProgress| {
            *cached.lock().expect("migration progress cache poisoned") =
                (progress.clone(), std::time::Instant::now());
        };
        let result = operation(&callback);
        let final_progress = cached
            .lock()
            .expect("migration progress cache poisoned")
            .0
            .clone();
        drop(stop);
        handle
            .join()
            .map_err(|_| "Migration progress worker failed.".to_string())?;
        emit(final_progress);
        result
    })
}

fn migrate_archive(
    state: &AppState,
    workspace_id: &str,
    destination: &str,
    label: &str,
    workers: usize,
    on_progress: impl Fn(&MigrationProgress) + Sync,
) -> Result<Workspace, String> {
    if destination.trim().is_empty() || label.trim().is_empty() {
        return Err("Choose a destination and archive name.".into());
    }
    MigrationOptions { workers }
        .validate()
        .map_err(|e| e.to_string())?;
    let source = source_workspace(state, workspace_id)?;
    let destination = PathBuf::from(destination.trim());
    with_source(state, Path::new(&source.store_path), |store| {
        if store.content.format_version() != 1 {
            return Err("This archive already uses FastCDC.".into());
        }
        state.update_config_transactionally(|config| {
            config
                .pending_migrations
                .retain(|pending| pending.workspace_id != workspace_id);
            config.pending_migrations.push(PendingMigration {
                workspace_id: workspace_id.to_owned(),
                destination: destination.to_string_lossy().into_owned(),
                label: label.trim().to_owned(),
            });
            Ok(())
        })?;
        migration::migrate_store_with_options(
            store,
            Path::new(&source.store_path),
            &destination,
            ChunkingProfile::default(),
            MigrationOptions { workers },
            on_progress,
            || state.migration_cancelled.load(Ordering::Relaxed),
        )
        .map_err(|e| format!("{e:#}"))?;
        Ok(())
    })?;
    let destination = destination.canonicalize().map_err(|e| e.to_string())?;
    let migrated = Store::open_existing(&destination).map_err(|e| format!("{e:#}"))?;
    let (
        total_files,
        total_dirs,
        unique_blobs,
        duplicate_files,
        total_original_bytes,
        total_stored_bytes,
    ) = migrated.compute_stats().map_err(|e| format!("{e:#}"))?;
    let ws = Workspace {
        id: workspace::generate_id(),
        label: label.trim().into(),
        tags: source.tags,
        store_path: destination.to_string_lossy().into_owned(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        stats: WorkspaceStats {
            total_files,
            total_dirs,
            unique_blobs,
            duplicate_files,
            total_original_bytes,
            total_stored_bytes,
            scans_count: source.stats.scans_count,
            last_scan_at: source.stats.last_scan_at,
        },
    };
    let config = state.update_config_transactionally(|config| {
        if !config.workspaces.iter().any(|existing| {
            Path::new(&existing.store_path).canonicalize().ok().as_ref() == Some(&destination)
        }) {
            config.workspaces.push(ws.clone());
        }
        config
            .pending_migrations
            .retain(|pending| pending.workspace_id != workspace_id);
        Ok(())
    })?;
    config.workspaces.into_iter().find(|ws| Path::new(&ws.store_path).canonicalize().ok().as_ref() == Some(&destination))
        .ok_or_else(|| "Migration finished, but the new archive could not be registered. Open its destination folder.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn ui_updates_continue_during_preparation_and_final_registration() {
        let events = std::sync::Mutex::new(Vec::new());
        with_progress_events(
            4,
            |p| events.lock().unwrap().push(p),
            |progress| {
                std::thread::sleep(std::time::Duration::from_millis(600));
                progress(&MigrationProgress {
                    phase: MigrationPhase::Completed,
                    unique_files: 1,
                    total_unique_files: 1,
                    ..Default::default()
                });
                std::thread::sleep(std::time::Duration::from_millis(600));
                Ok(())
            },
        )
        .unwrap();
        let events = events.into_inner().unwrap();
        assert!(events.iter().any(|p| p.phase == MigrationPhase::Preparing));
        assert!(events
            .iter()
            .any(|p| p.phase == MigrationPhase::Finalizing && p.elapsed_seconds >= 0.5));
        assert_eq!(events.last().unwrap().phase, MigrationPhase::Completed);
    }

    fn setup(root: &Path) -> AppState {
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("a"), b"first payload").unwrap();
        fs::write(source.join("b"), b"second payload").unwrap();
        fs::write(source.join("copy"), b"first payload").unwrap();
        let archive = root.join("archive");
        fs::create_dir_all(archive.join("blobs")).unwrap();
        let store = Store::open(&archive).unwrap();
        store.scan(&source).unwrap();
        let state = AppState::new(root.join("config.json"));
        *state.store_path.lock().unwrap() = archive.clone();
        *state.store.lock().unwrap() = Some(store);
        let mut config = state.workspaces.lock().unwrap();
        config.workspaces.push(Workspace {
            id: "original".into(),
            label: "Photos".into(),
            tags: vec!["backup".into()],
            store_path: archive.to_string_lossy().into_owned(),
            created_at: 0,
            stats: Default::default(),
        });
        config.active_workspace_id = Some("original".into());
        drop(config);
        state
    }

    #[test]
    fn migrates_a_cached_source_registers_the_result_and_keeps_original_active() {
        let root = TempDir::new().unwrap();
        let state = setup(root.path());
        let destination = root
            .path()
            .join("archive/fastcdc")
            .to_string_lossy()
            .into_owned();
        let job = MigrationJob::start(&state, "job-1").unwrap();
        assert!(OperationGuard::acquire(&state).is_err());
        let ws = migrate_archive(
            &state,
            "original",
            &destination,
            "Photos (FastCDC)",
            4,
            |_| {},
        )
        .unwrap();
        drop(job);
        assert!(!state.operation_running.load(Ordering::Relaxed));
        assert!(state.migration_job.lock().unwrap().is_none());
        let config = state.workspaces.lock().unwrap();
        assert_eq!(config.workspaces.len(), 2);
        assert_eq!(config.active_workspace_id.as_deref(), Some("original"));
        assert!(config.pending_migrations.is_empty());
        assert_eq!(ws.stats.total_files, 3);
        assert_eq!(ws.tags, vec!["backup"]);
        drop(config);
        let target = Store::open_existing(Path::new(&destination)).unwrap();
        assert_eq!(target.read_file("/a").unwrap(), b"first payload");
        assert_eq!(
            state
                .store
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .read_file("/a")
                .unwrap(),
            b"first payload"
        );
    }

    #[test]
    fn cancellation_persists_a_resume_location_and_stale_jobs_cannot_cancel() {
        let root = TempDir::new().unwrap();
        let state = setup(root.path());
        let destination = root.path().join("fastcdc").to_string_lossy().into_owned();
        let job = MigrationJob::start(&state, "job-1").unwrap();
        request_cancel(&state, "stale-job").unwrap();
        assert!(!state.migration_cancelled.load(Ordering::Relaxed));
        let result = migrate_archive(
            &state,
            "original",
            &destination,
            "Photos (FastCDC)",
            1,
            |progress| {
                if progress.unique_files == 1 {
                    request_cancel(&state, "job-1").unwrap();
                }
            },
        );
        assert!(result.unwrap_err().contains("migration cancelled"));
        drop(job);
        let saved = workspace::WorkspacesConfig::load(&state.config_path).unwrap();
        assert_eq!(saved.pending_migrations.len(), 1);
        assert_eq!(saved.pending_migrations[0].destination, destination);
        let job = MigrationJob::start(&state, "job-2").unwrap();
        request_cancel(&state, "job-1").unwrap();
        assert!(!state.migration_cancelled.load(Ordering::Relaxed));
        let resumed = std::sync::atomic::AtomicU64::new(0);
        migrate_archive(
            &state,
            "original",
            &destination,
            "Photos (FastCDC)",
            1,
            |p| resumed.store(p.resumed_files, Ordering::Relaxed),
        )
        .unwrap();
        assert_eq!(resumed.load(Ordering::Relaxed), 1);
        drop(job);
        assert!(workspace::WorkspacesConfig::load(&state.config_path)
            .unwrap()
            .pending_migrations
            .is_empty());
    }

    #[test]
    fn failed_registration_can_be_retried_without_duplicating_the_archive() {
        let root = TempDir::new().unwrap();
        let state = setup(root.path());
        let destination = root.path().join("fastcdc").to_string_lossy().into_owned();
        let job = MigrationJob::start(&state, "job-1").unwrap();
        let blocker = root.path().join(".config.json.tmp");
        let result = migrate_archive(
            &state,
            "original",
            &destination,
            "Photos (FastCDC)",
            1,
            |p| {
                if p.unique_files == p.total_unique_files && !blocker.exists() {
                    fs::create_dir(&blocker).unwrap();
                }
            },
        );
        assert!(result.is_err());
        drop(job);
        fs::remove_dir(blocker).unwrap();
        assert_eq!(state.workspaces.lock().unwrap().workspaces.len(), 1);
        let job = MigrationJob::start(&state, "job-2").unwrap();
        migrate_archive(
            &state,
            "original",
            &destination,
            "Photos (FastCDC)",
            4,
            |_| {},
        )
        .unwrap();
        drop(job);
        let job = MigrationJob::start(&state, "job-3").unwrap();
        migrate_archive(
            &state,
            "original",
            &destination,
            "Photos (FastCDC)",
            4,
            |_| {},
        )
        .unwrap();
        drop(job);
        assert_eq!(state.workspaces.lock().unwrap().workspaces.len(), 2);
    }
}
