# Git Directory Bundling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in per-scan option that stores each `.git` directory as one deterministic `.git.tar` blob instead of deduplicating its internal files.

**Architecture:** Add `ScanOptions` to `dedup-core`, route the existing scanner through option-aware entry points, and teach the scanner to archive `.git` directories while skipping descent into their children. Expose the option through the store wrapper, CLI scan command, Tauri command/API wrapper, and Svelte scan dialog.

**Tech Stack:** Rust workspace, `dedup-core`, `dedup-cli`, Tauri v2 command layer, Svelte 5 app, `walkdir`, `tar`, existing LZ4 content store.

---

## File Structure

- Modify `crates/dedup-core/Cargo.toml`
  - Add the `tar` crate used by production archive creation and core tests.
- Modify `crates/dedup-core/src/types.rs`
  - Add `ScanOptions`.
  - Re-export via `crates/dedup-core/src/lib.rs`.
- Modify `crates/dedup-core/src/scanner.rs`
  - Add option-aware scanner entry points.
  - Refactor file storage into a helper so regular files and `.git.tar` bundles share metadata/stat behavior.
  - Add deterministic `.git` tar creation.
  - Add core regression tests.
- Modify `crates/dedup-core/src/lib.rs`
  - Add store wrapper methods for scan options and scan options plus cancellation.
- Modify `crates/dedup-cli/src/main.rs`
  - Add `--bundle-git-dirs` to `dedup scan`.
  - Pass `ScanOptions` into the store scan wrapper.
- Modify `app/src-tauri/src/commands.rs`
  - Add a `bundle_git_dirs: bool` Tauri scan argument.
  - Pass `ScanOptions` into the cancellable scan wrapper.
- Modify `app/src/lib/api/tauri.ts`
  - Add the `bundleGitDirs` argument to `scanDirectory`.
- Modify `app/src/routes/+page.svelte`
  - Add unchecked per-scan checkbox state.
  - Pass the checkbox state into `scanDirectory`.

---

### Task 1: Add Core Scan Options API With Failing Tests

**Files:**
- Modify: `crates/dedup-core/src/types.rs`
- Modify: `crates/dedup-core/src/lib.rs`
- Modify: `crates/dedup-core/src/scanner.rs`

- [ ] **Step 1: Write failing tests for default and opt-in option surface**

Append these tests to the existing `#[cfg(test)] mod tests` in `crates/dedup-core/src/scanner.rs`:

```rust
    #[test]
    fn default_scan_still_indexes_git_directory_entries() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir(source_dir.path().join(".git")).unwrap();
        fs::write(source_dir.path().join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();
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
        fs::write(source_dir.path().join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();

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
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p dedup-core scan_options_default_does_not_bundle_git_directories
```

Expected: compile failure naming missing `ScanOptions` and `scan_directory_into_with_options`.

- [ ] **Step 3: Add `ScanOptions` type**

In `crates/dedup-core/src/types.rs`, after the `ScanProgress` struct, add:

```rust
/// Options controlling scan behavior.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct ScanOptions {
    /// Store each `.git` directory as one archive blob instead of scanning entries.
    pub bundle_git_dirs: bool,
}
```

In `crates/dedup-core/src/lib.rs`, update the re-export:

```rust
pub use types::{
    DirEntry, DirMetadata, ExtensionStats, FileMetadata, ScanOptions, ScanProgress, ScanStats,
};
```

In `crates/dedup-core/src/scanner.rs`, update the type import:

```rust
use crate::types::{DirMetadata, FileMetadata, ScanOptions, ScanProgress, ScanStats};
```

- [ ] **Step 4: Add option-aware scanner entry points with default behavior**

In `crates/dedup-core/src/scanner.rs`, replace the current `scan_directory_into` wrapper and `scan_directory_into_with_cancellation` signature block with this sequence:

```rust
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
```

Keep the existing scanner body inside `scan_directory_into_with_options_and_cancellation`. The `options` variable will be unused until Task 2.

- [ ] **Step 5: Run tests to verify the option API passes default behavior**

Run:

```bash
cargo test -p dedup-core default_scan_still_indexes_git_directory_entries scan_options_default_does_not_bundle_git_directories
```

Expected: both tests pass. If Rust rejects multiple test filters, run each test command separately.

- [ ] **Step 6: Commit Task 1**

```bash
git add crates/dedup-core/src/types.rs crates/dedup-core/src/lib.rs crates/dedup-core/src/scanner.rs
git commit -m "feat: add scan options API"
```

---

### Task 2: Implement Deterministic `.git` Directory Bundling In Core

**Files:**
- Modify: `crates/dedup-core/Cargo.toml`
- Modify: `crates/dedup-core/src/scanner.rs`

- [ ] **Step 1: Add tar dependency**

In `crates/dedup-core/Cargo.toml`, add this dependency under `[dependencies]`:

```toml
tar = "0.4"
```

- [ ] **Step 2: Write failing opt-in bundle test**

Append this test to `crates/dedup-core/src/scanner.rs`:

```rust
    #[test]
    fn bundle_git_dirs_stores_single_git_tar_file() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir(source_dir.path().join(".git")).unwrap();
        fs::write(source_dir.path().join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();
        fs::create_dir_all(source_dir.path().join(".git/refs/heads")).unwrap();
        fs::write(source_dir.path().join(".git/refs/heads/main"), b"abc123\n").unwrap();
        fs::write(source_dir.path().join("tracked.txt"), b"normal content").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                bundle_git_dirs: true,
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 2);
        assert!(metadata_db.get_file("/repo/tracked.txt").unwrap().is_some());
        assert!(metadata_db.get_file("/repo/.git.tar").unwrap().is_some());
        assert!(metadata_db.get_file("/repo/.git/HEAD").unwrap().is_none());
        assert!(metadata_db
            .get_file("/repo/.git/refs/heads/main")
            .unwrap()
            .is_none());

        let git_meta = metadata_db.get_file("/repo/.git.tar").unwrap().unwrap();
        let git_cid = cid_util::cid_from_bytes(&git_meta.cid).unwrap();
        let archive_bytes = content_store.read(&git_cid).unwrap();
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

        assert!(names.contains(&".git/HEAD".to_string()));
        assert!(names.contains(&".git/refs/heads/main".to_string()));
    }
```

- [ ] **Step 3: Run test to verify it fails**

Run:

```bash
cargo test -p dedup-core bundle_git_dirs_stores_single_git_tar_file
```

Expected: test compiles and fails because `/repo/.git.tar` is missing.

- [ ] **Step 4: Add imports and helper types**

In `crates/dedup-core/src/scanner.rs`, replace the imports at the top with:

```rust
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{bail, Context, Result};
use tar::{Builder as TarBuilder, EntryType, Header};
use walkdir::WalkDir;
```

- [ ] **Step 5: Add reusable file storage helper**

Add this helper above `ensure_parent_dirs` in `crates/dedup-core/src/scanner.rs`:

```rust
fn store_virtual_file<F>(
    virtual_path: String,
    data: Vec<u8>,
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

    on_progress(&ScanProgress {
        files_processed: stats.total_files,
        dirs_processed: stats.total_dirs,
        bytes_processed: stats.total_original_bytes,
        bytes_stored: stats.total_stored_bytes,
        duplicates_found: stats.duplicate_files,
        skipped_files: stats.skipped_files,
        current_file: virtual_path,
    });

    Ok(())
}
```

- [ ] **Step 6: Add deterministic tar helpers**

Add these helpers above `store_virtual_file`:

```rust
fn build_git_directory_archive<C>(git_dir: &Path, should_cancel: &C) -> Result<Vec<u8>>
where
    C: Fn() -> bool,
{
    let mut entries: Vec<PathBuf> = Vec::new();

    for entry in WalkDir::new(git_dir).follow_links(false).min_depth(1) {
        if should_cancel() {
            bail!("scan cancelled");
        }

        let entry = entry?;
        if entry.file_type().is_dir() || entry.file_type().is_file() {
            entries.push(entry.path().to_path_buf());
        }
    }

    entries.sort_by(|a, b| {
        archive_relative_path(git_dir, a).cmp(&archive_relative_path(git_dir, b))
    });

    let mut archive_bytes = Vec::new();
    {
        let mut builder = TarBuilder::new(&mut archive_bytes);

        for path in entries {
            if should_cancel() {
                bail!("scan cancelled");
            }

            let relative = archive_relative_path(git_dir, &path);
            let archive_path = Path::new(".git").join(relative);
            let meta = fs::metadata(&path)?;

            if meta.is_dir() {
                append_tar_directory(&mut builder, &archive_path)?;
            } else if meta.is_file() {
                append_tar_file(&mut builder, &path, &archive_path, meta.len())?;
            }
        }

        builder.finish().context("failed to finish .git tar archive")?;
    }

    Ok(archive_bytes)
}

fn archive_relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .collect::<PathBuf>()
}

fn append_tar_directory<W: Write>(builder: &mut TarBuilder<W>, path: &Path) -> Result<()> {
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Directory);
    header.set_size(0);
    header.set_mode(0o755);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_username("")?;
    header.set_groupname("")?;
    header.set_cksum();
    builder
        .append_data(&mut header, path, io::empty())
        .with_context(|| format!("failed to append directory to archive: {}", path.display()))
}

fn append_tar_file<W: Write>(
    builder: &mut TarBuilder<W>,
    source_path: &Path,
    archive_path: &Path,
    size: u64,
) -> Result<()> {
    let mut file = fs::File::open(source_path)
        .with_context(|| format!("failed to open archive source: {}", source_path.display()))?;
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_size(size);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_username("")?;
    header.set_groupname("")?;
    header.set_cksum();
    builder
        .append_data(&mut header, archive_path, &mut file)
        .with_context(|| format!("failed to append file to archive: {}", source_path.display()))
}
```

- [ ] **Step 7: Change the scanner loop to allow skipping `.git` descent**

In `scan_directory_into_with_options_and_cancellation`, replace:

```rust
    for entry in WalkDir::new(&source).follow_links(false) {
```

with:

```rust
    let mut walker = WalkDir::new(&source).follow_links(false).into_iter();
    while let Some(entry) = walker.next() {
```

- [ ] **Step 8: Add `.git` bundling branch before normal directory handling**

Inside the scanner loop, after `fs_meta` is read and before `if fs_meta.is_dir() {`, insert:

```rust
        if options.bundle_git_dirs
            && fs_meta.is_dir()
            && entry.file_name().to_string_lossy() == ".git"
        {
            let archive_path = format!("{virtual_path}.tar");
            match build_git_directory_archive(abs_path, &should_cancel) {
                Ok(data) => {
                    let modified = extract_mtime(&fs_meta);
                    let created = extract_ctime(&fs_meta);
                    store_virtual_file(
                        archive_path,
                        data,
                        modified,
                        created,
                        0o644,
                        content_store,
                        metadata_db,
                        &mut stats,
                        &on_progress,
                    )?;
                }
                Err(err) => {
                    if err.to_string().contains("scan cancelled") {
                        return Err(err);
                    }

                    let msg = format!("failed to archive .git directory: {err}");
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
                        current_file: archive_path,
                    });
                }
            }
            walker.skip_current_dir();
            continue;
        }
```

- [ ] **Step 9: Replace regular file storage block with helper call**

In the regular file branch, keep the `fs::read(abs_path)` error handling and replace the CID/store/FileMetadata/stat/progress code after `let data = match ...;` with:

```rust
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
                virtual_path,
                data,
                modified,
                created,
                permissions,
                content_store,
                metadata_db,
                &mut stats,
                &on_progress,
            )?;
```

- [ ] **Step 10: Extract creation time helper**

Below `extract_mtime`, add:

```rust
fn extract_ctime(meta: &fs::Metadata) -> i64 {
    meta.created()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
```

- [ ] **Step 11: Run opt-in test**

Run:

```bash
cargo test -p dedup-core bundle_git_dirs_stores_single_git_tar_file
```

Expected: pass.

- [ ] **Step 12: Run existing scanner tests**

Run:

```bash
cargo test -p dedup-core scanner::
```

Expected: all scanner tests pass.

- [ ] **Step 13: Commit Task 2**

```bash
git add crates/dedup-core/Cargo.toml crates/dedup-core/src/scanner.rs Cargo.lock
git commit -m "feat: bundle git directories during scans"
```

---

### Task 3: Add Duplicate Accounting And Cancellation Tests

**Files:**
- Modify: `crates/dedup-core/src/scanner.rs`

- [ ] **Step 1: Write duplicate accounting test**

Append this test to `crates/dedup-core/src/scanner.rs`:

```rust
    #[test]
    fn bundled_identical_git_dirs_are_counted_as_duplicates() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir_all(source_dir.path().join("one/.git/refs/heads")).unwrap();
        fs::write(source_dir.path().join("one/.git/HEAD"), b"ref: refs/heads/main\n").unwrap();
        fs::write(source_dir.path().join("one/.git/refs/heads/main"), b"abc123\n").unwrap();

        fs::create_dir_all(source_dir.path().join("two/.git/refs/heads")).unwrap();
        fs::write(source_dir.path().join("two/.git/HEAD"), b"ref: refs/heads/main\n").unwrap();
        fs::write(source_dir.path().join("two/.git/refs/heads/main"), b"abc123\n").unwrap();

        let stats = scan_directory_into_with_options(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                bundle_git_dirs: true,
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.unique_blobs, 1);
        assert_eq!(stats.duplicate_files, 1);
        assert!(metadata_db.get_file("/repo/one/.git.tar").unwrap().is_some());
        assert!(metadata_db.get_file("/repo/two/.git.tar").unwrap().is_some());
    }
```

- [ ] **Step 2: Run duplicate test**

Run:

```bash
cargo test -p dedup-core bundled_identical_git_dirs_are_counted_as_duplicates
```

Expected: pass if Task 2 produced deterministic archive bytes. If it fails with `unique_blobs == 2`, inspect tar metadata normalization before changing the assertion.

- [ ] **Step 3: Write cancellation test for git bundling**

Append this test to `crates/dedup-core/src/scanner.rs`:

```rust
    #[test]
    fn bundled_git_scan_honors_cancellation() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir_all(source_dir.path().join(".git/objects")).unwrap();
        for idx in 0..50 {
            fs::write(
                source_dir.path().join(format!(".git/objects/{idx:02}")),
                format!("object-{idx}"),
            )
            .unwrap();
        }

        use std::sync::atomic::{AtomicU64, Ordering};
        let cancellation_checks = AtomicU64::new(0);
        let result = scan_directory_into_with_options_and_cancellation(
            source_dir.path(),
            "/repo",
            store_dir.path(),
            &content_store,
            &metadata_db,
            ScanOptions {
                bundle_git_dirs: true,
            },
            |_| {},
            || {
                cancellation_checks.fetch_add(1, Ordering::Relaxed) >= 2
            },
        );

        assert!(result.unwrap_err().to_string().contains("scan cancelled"));
        assert!(cancellation_checks.load(Ordering::Relaxed) >= 3);
        assert!(metadata_db.get_file("/repo/.git.tar").unwrap().is_none());
    }
```

- [ ] **Step 4: Run cancellation test**

Run:

```bash
cargo test -p dedup-core bundled_git_scan_honors_cancellation
```

Expected: pass.

- [ ] **Step 5: Run all core tests**

Run:

```bash
cargo test -p dedup-core
```

Expected: all tests pass.

- [ ] **Step 6: Commit Task 3**

```bash
git add crates/dedup-core/src/scanner.rs
git commit -m "test: cover git directory bundling edge cases"
```

---

### Task 4: Wire Store, CLI, And Tauri Scan Options

**Files:**
- Modify: `crates/dedup-core/src/lib.rs`
- Modify: `crates/dedup-cli/src/main.rs`
- Modify: `app/src-tauri/src/commands.rs`

- [ ] **Step 1: Add store wrapper methods**

In `crates/dedup-core/src/lib.rs`, add `ScanOptions` to the import already re-exported in Task 1. Then add this method after `scan_into`:

```rust
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
```

Replace the body of `scan_into_with_cancellation` with:

```rust
        self.scan_into_with_options_and_cancellation(
            source,
            target_path,
            ScanOptions::default(),
            on_progress,
            should_cancel,
        )
```

Add this method after `scan_into_with_cancellation`:

```rust
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
```

- [ ] **Step 2: Wire CLI flag**

In `crates/dedup-cli/src/main.rs`, change the import:

```rust
use dedup_core::{ScanOptions, Store};
```

In the `Commands::Scan` variant, add:

```rust
        /// Bundle each .git directory into one .git.tar archive blob.
        #[arg(long)]
        bundle_git_dirs: bool,
```

Change the `match` arm:

```rust
        Commands::Scan {
            source,
            store,
            target,
            bundle_git_dirs,
        } => cmd_scan(&source, &store, &target, bundle_git_dirs),
```

Change the function signature:

```rust
fn cmd_scan(
    source: &PathBuf,
    store_path: &PathBuf,
    target: &str,
    bundle_git_dirs: bool,
) -> Result<()> {
```

After printing `Target`, add:

```rust
    if bundle_git_dirs {
        println!("Git dirs: bundled as .git.tar");
    }
```

Replace `store.scan_into(source, target, |progress| {` with:

```rust
    let stats = store.scan_into_with_options(
        source,
        target,
        ScanOptions { bundle_git_dirs },
        |progress| {
```

Close that call with:

```rust
        },
    ).context("scan failed")?;
```

- [ ] **Step 3: Wire Tauri command**

In `app/src-tauri/src/commands.rs`, change the import:

```rust
use dedup_core::{
    DirEntry, ExtensionStats, FileMetadata, ScanOptions, ScanProgress, ScanStats, Store,
};
```

Add `bundle_git_dirs: bool` to the `scan_directory` parameters after `target_path: String`.

Replace `.scan_into_with_cancellation(` with `.scan_into_with_options_and_cancellation(` and add options before the progress callback:

```rust
            .scan_into_with_options_and_cancellation(
                &source_path,
                &target_path,
                ScanOptions { bundle_git_dirs },
                move |progress: &ScanProgress| {
                    let _ = app.emit("scan-progress", progress.clone());
                },
                || cancel_flag.load(Ordering::Relaxed),
            )
```

- [ ] **Step 4: Run compile checks for Rust wiring**

Run:

```bash
cargo check
```

Expected: pass for `dedup-core`, `dedup-cli`, and `dedup-app`.

- [ ] **Step 5: Run CLI smoke command help**

Run:

```bash
cargo run -p dedup-cli -- scan --help
```

Expected: output includes `--bundle-git-dirs`.

- [ ] **Step 6: Commit Task 4**

```bash
git add crates/dedup-core/src/lib.rs crates/dedup-cli/src/main.rs app/src-tauri/src/commands.rs
git commit -m "feat: expose git bundling scan option"
```

---

### Task 5: Wire Svelte API And Scan Dialog

**Files:**
- Modify: `app/src/lib/api/tauri.ts`
- Modify: `app/src/routes/+page.svelte`

- [ ] **Step 1: Update API wrapper**

In `app/src/lib/api/tauri.ts`, change `scanDirectory` to:

```ts
export async function scanDirectory(
	source: string,
	targetPath: string,
	bundleGitDirs = false
): Promise<ScanStats> {
	return invoke('scan_directory', { source, targetPath, bundleGitDirs });
}
```

- [ ] **Step 2: Add Svelte state**

In `app/src/routes/+page.svelte`, after:

```ts
	let targetPath = $state('/');
```

add:

```ts
	let bundleGitDirs = $state(false);
```

- [ ] **Step 3: Reset checkbox for each scan dialog**

In `openScanDialog`, after:

```ts
		targetPath = presetTarget ?? '/';
```

add:

```ts
		bundleGitDirs = false;
```

- [ ] **Step 4: Pass the option into scanDirectory**

Replace:

```ts
			scanResult = await scanDirectory(scanSource, targetPath);
```

with:

```ts
			scanResult = await scanDirectory(scanSource, targetPath, bundleGitDirs);
```

- [ ] **Step 5: Add unchecked checkbox to scan dialog**

In `app/src/routes/+page.svelte`, after the virtual path `<label>` block and before the progress section, add:

```svelte
				<label class="checkbox-row">
					<input type="checkbox" bind:checked={bundleGitDirs} disabled={scanning} />
					<span>Bundle .git directories</span>
				</label>
```

Add this CSS near the other dialog label/input styles:

```css
	.checkbox-row {
		flex-direction: row !important;
		align-items: center;
		gap: 8px !important;
	}

	.checkbox-row input {
		width: 14px;
		height: 14px;
		padding: 0;
		margin: 0;
	}
```

- [ ] **Step 6: Run Svelte type check**

Run:

```bash
npm --prefix app run check
```

Expected: `svelte-check found 0 errors and 0 warnings`.

- [ ] **Step 7: Commit Task 5**

```bash
git add app/src/lib/api/tauri.ts app/src/routes/+page.svelte
git commit -m "feat: add git bundling scan checkbox"
```

---

### Task 6: Final Verification

**Files:**
- No planned file changes unless verification exposes a defect.

- [ ] **Step 1: Run core tests**

Run:

```bash
cargo test -p dedup-core
```

Expected: all tests pass, including:
- `default_scan_still_indexes_git_directory_entries`
- `scan_options_default_does_not_bundle_git_directories`
- `bundle_git_dirs_stores_single_git_tar_file`
- `bundled_identical_git_dirs_are_counted_as_duplicates`
- `bundled_git_scan_honors_cancellation`

- [ ] **Step 2: Run full Rust workspace check**

Run:

```bash
cargo check
```

Expected: pass.

- [ ] **Step 3: Run frontend type check**

Run:

```bash
npm --prefix app run check
```

Expected: `svelte-check found 0 errors and 0 warnings`.

- [ ] **Step 4: Inspect final diff**

Run:

```bash
git status --short
git diff --stat HEAD~5..HEAD
```

Expected: only files from this plan changed across the task commits.

- [ ] **Step 5: Commit verification fixes if needed**

If Step 1, 2, or 3 required fixes, inspect the changed files with:

```bash
git status --short
git diff --stat
```

Then stage only files already listed in this plan, using the applicable command:

```bash
git add crates/dedup-core/Cargo.toml crates/dedup-core/src/types.rs crates/dedup-core/src/lib.rs crates/dedup-core/src/scanner.rs Cargo.lock
git add crates/dedup-cli/src/main.rs app/src-tauri/src/commands.rs
git add app/src/lib/api/tauri.ts app/src/routes/+page.svelte
git commit -m "fix: stabilize git directory bundling"
```

If no fixes were needed, do not create an empty commit.
