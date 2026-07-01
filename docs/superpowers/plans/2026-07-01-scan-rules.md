# Scan Rules Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add opt-in per-scan rules that either ignore matched paths or archive matched directories, with built-in presets and saved app-level custom regex rules.

**Architecture:** `dedup-core` owns rule types, preset construction, regex validation, and scanner behavior. CLI and Tauri pass explicit `ScanRule` values per scan. The app stores reusable custom rules in `WorkspacesConfig` and the Svelte scan dialog builds the current scan's rule list from selected presets and enabled saved rules.

**Tech Stack:** Rust, `regex`, `walkdir`, deterministic `tar` archives, Tauri commands, Svelte 5, TypeScript.

---

## File Structure

- Modify `crates/dedup-core/Cargo.toml`: add `regex`.
- Modify `crates/dedup-core/src/types.rs`: add `ScanRule`, `ScanRuleAction`, and `BuiltinScanPreset`; extend `ScanOptions`.
- Modify `crates/dedup-core/src/lib.rs`: keep re-exports current.
- Modify `crates/dedup-core/src/scanner.rs`: replace `.git` special-case matching with generic compiled rules.
- Modify `crates/dedup-cli/src/main.rs`: add preset flags and repeated regex flags.
- Modify `app/src-tauri/src/workspace.rs`: add saved app-level `CustomScanRule`.
- Modify `app/src-tauri/src/commands.rs`: accept scan rules, expose rule-management commands, and preserve `bundle_git_dirs` compatibility.
- Modify `app/src-tauri/src/main.rs`: register new rule-management commands.
- Modify `app/src/lib/api/tauri.ts`: add rule types and rule-management API calls.
- Modify `app/src/routes/+page.svelte`: add preset checkboxes, saved custom rule management, and scan-rule construction.

---

### Task 1: Add Core Rule Types And Presets

**Files:**
- Modify: `crates/dedup-core/Cargo.toml`
- Modify: `crates/dedup-core/src/types.rs`
- Modify: `crates/dedup-core/src/lib.rs`

- [ ] **Step 1: Add failing type tests**

Append these tests to the existing `#[cfg(test)] mod tests` block in `crates/dedup-core/src/scanner.rs`. If the module does not currently import these names, extend its `use crate::types::{...};` or use fully qualified names.

```rust
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
```

- [ ] **Step 2: Run tests and confirm failure**

Run:

```bash
cargo test -p dedup-core builtin_scan_presets_expand_to_rules scan_options_default_has_no_rules
```

Expected: compilation fails because `ScanRule`, `ScanRuleAction`, and `BuiltinScanPreset` do not exist yet.

- [ ] **Step 3: Add `regex` dependency**

In `crates/dedup-core/Cargo.toml`, add under `[dependencies]`:

```toml
regex = "1"
```

- [ ] **Step 4: Add rule types**

In `crates/dedup-core/src/types.rs`, replace the current `ScanOptions` block with:

```rust
/// Action to apply when a scan rule matches a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanRuleAction {
    /// Skip matched files and directories.
    Ignore,
    /// Store a matched directory as one deterministic `.tar` file.
    Archive,
}

/// Built-in scan rule presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinScanPreset {
    Git,
    RustTarget,
    NodeModules,
    PythonVenv,
}

/// A regex-based scan rule matched against the full scan-relative path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanRule {
    pub pattern: String,
    pub action: ScanRuleAction,
}

impl ScanRule {
    pub fn new(pattern: impl Into<String>, action: ScanRuleAction) -> Self {
        Self {
            pattern: pattern.into(),
            action,
        }
    }

    pub fn builtin(preset: BuiltinScanPreset) -> Self {
        match preset {
            BuiltinScanPreset::Git => Self::new(r"(^|/)\.git$", ScanRuleAction::Archive),
            BuiltinScanPreset::RustTarget => Self::new(r"(^|/)target$", ScanRuleAction::Ignore),
            BuiltinScanPreset::NodeModules => Self::new(r"(^|/)node_modules$", ScanRuleAction::Ignore),
            BuiltinScanPreset::PythonVenv => {
                Self::new(r"(^|/)(\.venv|venv)$", ScanRuleAction::Ignore)
            }
        }
    }
}

/// Options controlling scan behavior.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanOptions {
    /// Compatibility flag for storing each `.git` directory as one archive blob.
    pub bundle_git_dirs: bool,
    /// Ordered regex rules. First match wins.
    pub rules: Vec<ScanRule>,
}
```

- [ ] **Step 5: Re-export new types**

In `crates/dedup-core/src/lib.rs`, update the `pub use types::{...};` block to include the new names:

```rust
pub use types::{
    BuiltinScanPreset, DirEntry, DirMetadata, ExtensionStats, FileMetadata, ScanOptions,
    ScanProgress, ScanRule, ScanRuleAction, ScanStats,
};
```

- [ ] **Step 6: Run tests and fix formatting only in touched lines**

Run:

```bash
cargo test -p dedup-core builtin_scan_presets_expand_to_rules
cargo test -p dedup-core scan_options_default_has_no_rules
cargo test -p dedup-core
git show --check HEAD
```

Expected: all commands pass. If the full test suite fails because later scanner code still expects `ScanOptions: Copy`, change any pass-by-value call sites to clone options only where needed; do not reintroduce `Copy`.

- [ ] **Step 7: Commit**

```bash
git add crates/dedup-core/Cargo.toml crates/dedup-core/src/types.rs crates/dedup-core/src/lib.rs crates/dedup-core/src/scanner.rs
git commit -m "feat: add scan rule types"
```

---

### Task 2: Implement Generic Core Rule Evaluation

**Files:**
- Modify: `crates/dedup-core/src/scanner.rs`

- [ ] **Step 1: Add failing ignore and invalid-regex tests**

Add these tests to `crates/dedup-core/src/scanner.rs`:

```rust
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
        assert!(metadata_db.get_file("/repo/target/debug/app").unwrap().is_none());
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
```

- [ ] **Step 2: Run tests and confirm failure**

Run:

```bash
cargo test -p dedup-core ignore_rule_skips_matching_directory_and_children
cargo test -p dedup-core invalid_scan_rule_regex_fails_before_storing_metadata
```

Expected: first test fails because rules are not evaluated yet; second test fails because invalid regex is not compiled before scanning.

- [ ] **Step 3: Add compiled rule helpers**

In `crates/dedup-core/src/scanner.rs`, update imports:

```rust
use regex::Regex;

use crate::types::{
    BuiltinScanPreset, DirMetadata, FileMetadata, ScanOptions, ScanProgress, ScanRule,
    ScanRuleAction, ScanStats,
};
```

Add these helpers above `scan_directory_into_with_options_and_cancellation`:

```rust
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

fn matching_rule<'a>(rules: &'a [CompiledScanRule], relative_path: &str) -> Option<&'a CompiledScanRule> {
    rules.iter().find(|rule| rule.regex.is_match(relative_path))
}

fn emit_skipped_progress<F>(
    virtual_path: String,
    stats: &ScanStats,
    on_progress: &F,
)
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
```

- [ ] **Step 4: Compile rules before filesystem mutation**

Inside `scan_directory_into_with_options_and_cancellation`, immediately after canonicalizing `source`, add:

```rust
    let compiled_rules = compile_scan_rules(&options)?;
```

This must happen before `ensure_parent_dirs(metadata_db, &prefix)?;` so invalid regexes do not create target metadata.

- [ ] **Step 5: Apply ignore rules before `.git` compatibility collision handling**

Inside the scanner loop, after `virtual_path` is computed and before the current `.git.tar` collision block, add:

```rust
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
```

This is temporary; Task 3 replaces the `Archive` branch with real archive storage.

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test -p dedup-core ignore_rule_skips_matching_directory_and_children
cargo test -p dedup-core invalid_scan_rule_regex_fails_before_storing_metadata
```

Expected: both tests pass.

- [ ] **Step 7: Run regression tests**

Run:

```bash
cargo test -p dedup-core default_scan_still_indexes_git_directory_entries
cargo test -p dedup-core bundle_git_dirs_stores_single_git_tar_file
cargo test -p dedup-core
git show --check HEAD
```

Expected: all pass. If `.git` bundling tests fail because generic `Archive` branch now intercepts `.git`, proceed to Task 3 before committing; otherwise commit now.

- [ ] **Step 8: Commit**

```bash
git add crates/dedup-core/src/scanner.rs
git commit -m "feat: ignore paths with scan rules"
```

---

### Task 3: Generalize Archive Rules And Preserve `.git` Behavior

**Files:**
- Modify: `crates/dedup-core/src/scanner.rs`

- [ ] **Step 1: Add failing archive and order tests**

Add these tests to `crates/dedup-core/src/scanner.rs`:

```rust
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
        assert!(metadata_db.get_file("/repo/cache/sub/item").unwrap().is_none());
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

        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.skipped_files, 1);
        assert!(metadata_db.get_file("/repo/cache.tar").unwrap().is_none());
    }
```

- [ ] **Step 2: Run tests and confirm failure**

Run:

```bash
cargo test -p dedup-core archive_rule_stores_matching_directory_as_tar
cargo test -p dedup-core archive_rule_matching_regular_file_skips_it
cargo test -p dedup-core scan_rules_use_first_match
```

Expected: archive directory test fails because archive rules are not storing directories yet.

- [ ] **Step 3: Generalize archive builder naming**

In `crates/dedup-core/src/scanner.rs`, rename `build_git_directory_archive` to:

```rust
fn build_directory_archive<C>(archive_root_name: &str, dir: &Path, should_cancel: &C) -> Result<Vec<u8>>
where
    C: Fn() -> bool,
```

Inside it, replace `git_dir` with `dir`, and replace `format!(".git/{relative}")` with:

```rust
archive_path: format!("{archive_root_name}/{relative}"),
```

Update call sites:

```rust
build_directory_archive(".git", abs_path, &should_cancel)
```

The existing `build_git_archive_observes_cancellation_while_streaming_file` test should call:

```rust
let result = build_directory_archive(".git", &git_dir, &|| {
    cancel_checks.fetch_add(1, Ordering::SeqCst) >= 3
});
```

- [ ] **Step 4: Add helper for archive root names**

Add this helper near `archive_relative_path`/archive helpers:

```rust
fn archive_root_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| "archive".to_string())
}
```

- [ ] **Step 5: Replace archive rule branch with real archive storage**

Replace the temporary `ScanRuleAction::Archive` branch from Task 2 with:

```rust
                ScanRuleAction::Archive => {
                    if !entry_file_type.is_dir() {
                        stats.skipped_files += 1;
                        emit_skipped_progress(virtual_path, &stats, &on_progress);
                        continue;
                    }

                    let archive_virtual_path = format!("{virtual_path}.tar");
                    let root_name = archive_root_name(abs_path);
                    let archive_data = match build_directory_archive(&root_name, abs_path, &should_cancel) {
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
                        &mut stats,
                        &on_progress,
                    )?;

                    walker.skip_current_dir();
                    continue;
                }
```

- [ ] **Step 6: Remove or bypass the old `.git` special-case branch**

Delete the old `if options.bundle_git_dirs && entry_file_type.is_dir() && ... ".git"` block. Keep the `.git.tar` collision helper only if a test still needs it; otherwise replace it with generic archive collision handling in Task 4.

After deletion, `.git` archive behavior must be driven by `compile_scan_rules` adding `ScanRule::builtin(BuiltinScanPreset::Git)` when `bundle_git_dirs` is true.

- [ ] **Step 7: Run archive tests**

Run:

```bash
cargo test -p dedup-core archive_rule_stores_matching_directory_as_tar
cargo test -p dedup-core archive_rule_matching_regular_file_skips_it
cargo test -p dedup-core scan_rules_use_first_match
cargo test -p dedup-core bundle_git_dirs_stores_single_git_tar_file
cargo test -p dedup-core bundled_identical_git_dirs_are_counted_as_duplicates
cargo test -p dedup-core bundled_git_scan_honors_cancellation
```

Expected: all pass.

- [ ] **Step 8: Run full core tests**

Run:

```bash
cargo test -p dedup-core
git show --check HEAD
```

Expected: all pass.

- [ ] **Step 9: Commit**

```bash
git add crates/dedup-core/src/scanner.rs
git commit -m "feat: archive directories with scan rules"
```

---

### Task 4: Add Built-In Ignore Preset Tests And Collision Handling

**Files:**
- Modify: `crates/dedup-core/src/scanner.rs`

- [ ] **Step 1: Add built-in preset tests**

Add these tests to `crates/dedup-core/src/scanner.rs`:

```rust
    #[test]
    fn builtin_ignore_presets_skip_common_project_artifacts() {
        let (source_dir, store_dir, content_store, metadata_db) = setup_test_store();

        fs::create_dir_all(source_dir.path().join("target/debug")).unwrap();
        fs::write(source_dir.path().join("target/debug/app"), b"rust").unwrap();
        fs::create_dir_all(source_dir.path().join("app/node_modules/react")).unwrap();
        fs::write(source_dir.path().join("app/node_modules/react/index.js"), b"node").unwrap();
        fs::create_dir_all(source_dir.path().join("service/.venv/bin")).unwrap();
        fs::write(source_dir.path().join("service/.venv/bin/python"), b"python").unwrap();
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
        assert!(metadata_db.get_file("/repo/target/debug/app").unwrap().is_none());
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
```

- [ ] **Step 2: Add archive collision test**

Add this test:

```rust
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
```

- [ ] **Step 3: Run tests and confirm collision test fails if not handled**

Run:

```bash
cargo test -p dedup-core builtin_ignore_presets_skip_common_project_artifacts
cargo test -p dedup-core archive_rule_skips_real_file_colliding_with_archive_path
```

Expected: preset test passes if Task 3 is complete; collision test may fail if real `cache.tar` overwrites archive metadata.

- [ ] **Step 4: Add generic archive collision helpers**

Add these helpers near `has_real_sibling_git_dir_for_git_tar_collision` and then delete the `.git`-specific helper when unused:

```rust
fn archive_collision_source_relative(relative_path: &str) -> Option<String> {
    relative_path
        .strip_suffix(".tar")
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
```

- [ ] **Step 5: Apply collision skip before normal handling**

Replace the existing `.git.tar` collision block condition with a generic condition:

```rust
        if has_sibling_archive_source_for_rule(&source, &rel_str, &compiled_rules) {
            let msg = "virtual path collision: .tar is reserved for archived directory".to_string();
            (|| log_error!(&mut error_log, error_log_path, msg, virtual_path.clone()))();
            stats.skipped_files += 1;
            emit_skipped_progress(virtual_path, &stats, &on_progress);
            if entry_file_type.is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }
```

- [ ] **Step 6: Run full core suite**

Run:

```bash
cargo test -p dedup-core
git show --check HEAD
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/dedup-core/src/scanner.rs
git commit -m "test: cover scan rule presets and archive collisions"
```

---

### Task 5: Wire Rules Through Store, CLI, And Tauri Scan

**Files:**
- Modify: `crates/dedup-cli/src/main.rs`
- Modify: `app/src-tauri/src/commands.rs`
- Modify: `app/src/lib/api/tauri.ts`

- [ ] **Step 1: Update CLI imports**

In `crates/dedup-cli/src/main.rs`, change imports:

```rust
use dedup_core::{BuiltinScanPreset, ScanOptions, ScanRule, ScanRuleAction, Store};
```

- [ ] **Step 2: Add CLI scan flags**

In `Commands::Scan`, after `bundle_git_dirs`, add:

```rust
        /// Ignore directories named target.
        #[arg(long)]
        ignore_rust_target: bool,

        /// Ignore directories named node_modules.
        #[arg(long)]
        ignore_node_modules: bool,

        /// Ignore directories named .venv or venv.
        #[arg(long)]
        ignore_python_venv: bool,

        /// Ignore paths matching this full scan-relative regex. Repeatable.
        #[arg(long = "ignore-regex")]
        ignore_regexes: Vec<String>,

        /// Archive directories matching this full scan-relative regex. Repeatable.
        #[arg(long = "archive-regex")]
        archive_regexes: Vec<String>,
```

- [ ] **Step 3: Add helper to build CLI rules**

Add this helper above `cmd_scan`:

```rust
#[derive(Debug, Clone, Copy)]
struct PresetFlags {
    bundle_git_dirs: bool,
    ignore_rust_target: bool,
    ignore_node_modules: bool,
    ignore_python_venv: bool,
}

fn build_scan_rules(
    presets: PresetFlags,
    ignore_regexes: &[String],
    archive_regexes: &[String],
) -> Vec<ScanRule> {
    let mut rules = Vec::new();

    if presets.bundle_git_dirs {
        rules.push(ScanRule::builtin(BuiltinScanPreset::Git));
    }
    if presets.ignore_rust_target {
        rules.push(ScanRule::builtin(BuiltinScanPreset::RustTarget));
    }
    if presets.ignore_node_modules {
        rules.push(ScanRule::builtin(BuiltinScanPreset::NodeModules));
    }
    if presets.ignore_python_venv {
        rules.push(ScanRule::builtin(BuiltinScanPreset::PythonVenv));
    }

    rules.extend(
        ignore_regexes
            .iter()
            .cloned()
            .map(|pattern| ScanRule::new(pattern, ScanRuleAction::Ignore)),
    );
    rules.extend(
        archive_regexes
            .iter()
            .cloned()
            .map(|pattern| ScanRule::new(pattern, ScanRuleAction::Archive)),
    );

    rules
}
```

- [ ] **Step 4: Thread CLI fields**

Change the `Commands::Scan` match arm to:

```rust
        Commands::Scan {
            source,
            store,
            target,
            bundle_git_dirs,
            ignore_rust_target,
            ignore_node_modules,
            ignore_python_venv,
            ignore_regexes,
            archive_regexes,
        } => cmd_scan(
            &source,
            &store,
            &target,
            PresetFlags {
                bundle_git_dirs,
                ignore_rust_target,
                ignore_node_modules,
                ignore_python_venv,
            },
            &ignore_regexes,
            &archive_regexes,
        ),
```

Change `cmd_scan` signature:

```rust
fn cmd_scan(
    source: &PathBuf,
    store_path: &PathBuf,
    target: &str,
    presets: PresetFlags,
    ignore_regexes: &[String],
    archive_regexes: &[String],
) -> Result<()> {
```

- [ ] **Step 5: Use rules in CLI scan options**

Inside `cmd_scan`, replace the current `if bundle_git_dirs` print block with:

```rust
    if presets.bundle_git_dirs {
        println!("Git dirs: archived as .git.tar");
    }
    if presets.ignore_rust_target {
        println!("Rust target: ignored");
    }
    if presets.ignore_node_modules {
        println!("Node modules: ignored");
    }
    if presets.ignore_python_venv {
        println!("Python virtual envs: ignored");
    }
    for pattern in ignore_regexes {
        println!("Ignore regex: {pattern}");
    }
    for pattern in archive_regexes {
        println!("Archive regex: {pattern}");
    }
```

Before calling `store.scan_into_with_options`, add:

```rust
    let rules = build_scan_rules(presets, ignore_regexes, archive_regexes);
```

Replace `ScanOptions { bundle_git_dirs, ..ScanOptions::default() }` with:

```rust
            ScanOptions {
                bundle_git_dirs: false,
                rules,
            },
```

- [ ] **Step 6: Update Tauri scan command signature**

In `app/src-tauri/src/commands.rs`, import rule types:

```rust
use dedup_core::{
    DirEntry, ExtensionStats, FileMetadata, ScanOptions, ScanProgress, ScanRule, ScanStats, Store,
};
```

Change `scan_directory` parameters to:

```rust
    bundle_git_dirs: Option<bool>,
    rules: Option<Vec<ScanRule>>,
```

After `let bundle_git_dirs = bundle_git_dirs.unwrap_or(false);`, add:

```rust
    let mut rules = rules.unwrap_or_default();
    if bundle_git_dirs {
        rules.insert(0, ScanRule::builtin(dedup_core::BuiltinScanPreset::Git));
    }
```

Replace scan options construction with:

```rust
                ScanOptions {
                    bundle_git_dirs: false,
                    rules,
                },
```

- [ ] **Step 7: Update TypeScript scan API**

In `app/src/lib/api/tauri.ts`, add:

```ts
export type ScanRuleAction = 'ignore' | 'archive';

export interface ScanRule {
	pattern: string;
	action: ScanRuleAction;
}
```

Change `scanDirectory` to:

```ts
export async function scanDirectory(
	source: string,
	targetPath: string,
	bundleGitDirs = false,
	rules: ScanRule[] = []
): Promise<ScanStats> {
	return invoke('scan_directory', { source, targetPath, bundleGitDirs, rules });
}
```

- [ ] **Step 8: Run checks**

Run:

```bash
cargo check
cargo run -p dedup-cli -- scan --help
npm --prefix app run check
git show --check HEAD
```

Expected: CLI help includes `--ignore-rust-target`, `--ignore-node-modules`, `--ignore-python-venv`, `--ignore-regex`, and `--archive-regex`; all checks pass.

- [ ] **Step 9: Commit**

```bash
git add crates/dedup-cli/src/main.rs app/src-tauri/src/commands.rs app/src/lib/api/tauri.ts
git commit -m "feat: expose scan rules through commands"
```

---

### Task 6: Persist App-Level Custom Rules

**Files:**
- Modify: `app/src-tauri/Cargo.toml`
- Modify: `app/src-tauri/src/workspace.rs`
- Modify: `app/src-tauri/src/commands.rs`
- Modify: `app/src-tauri/src/main.rs`
- Modify: `app/src/lib/api/tauri.ts`

- [ ] **Step 1: Add backend custom rule types**

In `app/src-tauri/src/workspace.rs`, add near `WorkspacesConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomScanRuleAction {
    Ignore,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomScanRule {
    pub id: String,
    pub label: String,
    pub pattern: String,
    pub action: CustomScanRuleAction,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}
```

Update `WorkspacesConfig`:

```rust
pub struct WorkspacesConfig {
    pub workspaces: Vec<Workspace>,
    /// ID of the currently active workspace (if any).
    pub active_workspace_id: Option<String>,
    #[serde(default)]
    pub custom_scan_rules: Vec<CustomScanRule>,
}
```

- [ ] **Step 2: Add validation helper**

In `app/src-tauri/src/commands.rs`, add imports:

```rust
use regex::Regex;
use crate::workspace::CustomScanRule;
```

In `app/src-tauri/Cargo.toml`, add under `[dependencies]`:

```toml
regex = "1"
```

Add helper near workspace commands:

```rust
fn validate_custom_scan_rule(rule: &CustomScanRule) -> Result<(), String> {
    if rule.label.trim().is_empty() {
        return Err("Rule label cannot be empty.".to_string());
    }
    if rule.pattern.trim().is_empty() {
        return Err("Rule regex cannot be empty.".to_string());
    }
    Regex::new(&rule.pattern)
        .map(|_| ())
        .map_err(|e| format!("Invalid regex: {e}"))
}
```

- [ ] **Step 3: Add commands**

In `app/src-tauri/src/commands.rs`, add:

```rust
#[tauri::command]
pub fn list_custom_scan_rules(state: State<'_, AppState>) -> Result<Vec<CustomScanRule>, String> {
    let config = state.workspaces.lock().map_err(|e| e.to_string())?;
    Ok(config.custom_scan_rules.clone())
}

#[tauri::command]
pub fn save_custom_scan_rules(
    state: State<'_, AppState>,
    rules: Vec<CustomScanRule>,
) -> Result<Vec<CustomScanRule>, String> {
    for rule in &rules {
        validate_custom_scan_rule(rule)?;
    }

    {
        let mut config = state.workspaces.lock().map_err(|e| e.to_string())?;
        config.custom_scan_rules = rules;
    }

    state.save_config()?;

    let config = state.workspaces.lock().map_err(|e| e.to_string())?;
    Ok(config.custom_scan_rules.clone())
}
```

- [ ] **Step 4: Register commands**

In `app/src-tauri/src/main.rs`, add to `tauri::generate_handler![...]`:

```rust
            commands::list_custom_scan_rules,
            commands::save_custom_scan_rules,
```

- [ ] **Step 5: Add frontend API**

In `app/src/lib/api/tauri.ts`, add:

```ts
export interface CustomScanRule {
	id: string;
	label: string;
	pattern: string;
	action: ScanRuleAction;
	enabled: boolean;
}

export async function listCustomScanRules(): Promise<CustomScanRule[]> {
	return invoke('list_custom_scan_rules');
}

export async function saveCustomScanRules(rules: CustomScanRule[]): Promise<CustomScanRule[]> {
	return invoke('save_custom_scan_rules', { rules });
}
```

Update `WorkspacesConfig` interface:

```ts
export interface WorkspacesConfig {
	workspaces: Workspace[];
	active_workspace_id: string | null;
	custom_scan_rules: CustomScanRule[];
}
```

- [ ] **Step 6: Run checks**

Run:

```bash
cargo check
npm --prefix app run check
git show --check HEAD
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add app/src-tauri/Cargo.toml app/src-tauri/src/workspace.rs app/src-tauri/src/commands.rs app/src-tauri/src/main.rs app/src/lib/api/tauri.ts
git commit -m "feat: persist custom scan rules"
```

---

### Task 7: Add Scan Dialog Presets And Custom Rule UI

**Files:**
- Modify: `app/src/routes/+page.svelte`
- Modify: `app/src/lib/api/tauri.ts`

- [ ] **Step 1: Add imports and state**

In `app/src/routes/+page.svelte`, extend the API import:

```ts
	listCustomScanRules,
	saveCustomScanRules,
	type CustomScanRule,
	type ScanRule,
```

After `let bundleGitDirs = $state(false);`, add:

```ts
	let ignoreRustTarget = $state(false);
	let ignoreNodeModules = $state(false);
	let ignorePythonVenv = $state(false);
	let customScanRules = $state<CustomScanRule[]>([]);
	let activeCustomRuleIds = $state<string[]>([]);
	let customRulesError = $state<string | null>(null);
	let newRuleLabel = $state('');
	let newRulePattern = $state('');
	let newRuleAction = $state<'ignore' | 'archive'>('ignore');
```

- [ ] **Step 2: Load custom rules on mount**

In the existing `$effect(() => { loadWorkspaces(); });`, add:

```ts
		loadCustomScanRules();
```

Add function:

```ts
	async function loadCustomScanRules() {
		try {
			customScanRules = await listCustomScanRules();
			activeCustomRuleIds = customScanRules.filter((rule) => rule.enabled).map((rule) => rule.id);
			customRulesError = null;
		} catch (e) {
			customRulesError = String(e);
		}
	}
```

- [ ] **Step 3: Reset per-scan presets**

In `openScanDialog`, after `bundleGitDirs = false;`, add:

```ts
		ignoreRustTarget = false;
		ignoreNodeModules = false;
		ignorePythonVenv = false;
		activeCustomRuleIds = customScanRules.filter((rule) => rule.enabled).map((rule) => rule.id);
```

- [ ] **Step 4: Build scan rules**

Add helper:

```ts
	function buildScanRules(): ScanRule[] {
		const rules: ScanRule[] = [];
		if (bundleGitDirs) {
			rules.push({ pattern: '(^|/)\\.git$', action: 'archive' });
		}
		if (ignoreRustTarget) {
			rules.push({ pattern: '(^|/)target$', action: 'ignore' });
		}
		if (ignoreNodeModules) {
			rules.push({ pattern: '(^|/)node_modules$', action: 'ignore' });
		}
		if (ignorePythonVenv) {
			rules.push({ pattern: '(^|/)(\\.venv|venv)$', action: 'ignore' });
		}
		const activeCustomRules = new Set(activeCustomRuleIds);
		for (const rule of customScanRules) {
			if (activeCustomRules.has(rule.id)) {
				rules.push({ pattern: rule.pattern, action: rule.action });
			}
		}
		return rules;
	}

	function toggleCustomRule(ruleId: string, checked: boolean) {
		if (checked) {
			activeCustomRuleIds = Array.from(new Set([...activeCustomRuleIds, ruleId]));
		} else {
			activeCustomRuleIds = activeCustomRuleIds.filter((id) => id !== ruleId);
		}
	}
```

Change scan call:

```ts
				scanResult = await scanDirectory(scanSource, targetPath, false, buildScanRules());
```

- [ ] **Step 5: Add preset checkbox UI**

Replace the existing `.git` checkbox with:

```svelte
				<div class="scan-rules">
					<label class="checkbox-row">
						<input type="checkbox" bind:checked={bundleGitDirs} disabled={scanning} />
						<span>Archive .git directories</span>
					</label>
					<label class="checkbox-row">
						<input type="checkbox" bind:checked={ignoreRustTarget} disabled={scanning} />
						<span>Ignore Rust target directories</span>
					</label>
					<label class="checkbox-row">
						<input type="checkbox" bind:checked={ignoreNodeModules} disabled={scanning} />
						<span>Ignore node_modules directories</span>
					</label>
					<label class="checkbox-row">
						<input type="checkbox" bind:checked={ignorePythonVenv} disabled={scanning} />
						<span>Ignore Python virtual environments</span>
					</label>
				</div>
```

- [ ] **Step 6: Add saved custom rules UI**

After the preset block and before progress, add:

```svelte
				{#if customScanRules.length > 0}
					<div class="scan-rules">
						{#each customScanRules as rule (rule.id)}
							<label class="checkbox-row">
								<input
									type="checkbox"
									checked={activeCustomRuleIds.includes(rule.id)}
									onchange={(event) =>
										toggleCustomRule(rule.id, event.currentTarget.checked)}
									disabled={scanning}
								/>
								<span>{rule.label}</span>
								<button
									class="rule-remove"
									onclick={() => handleRemoveCustomRule(rule.id)}
									disabled={scanning}
								>
									Remove
								</button>
							</label>
						{/each}
					</div>
				{/if}
```

Do not save checkbox changes from the scan dialog in this task; they are per-current UI state.

- [ ] **Step 7: Add minimal saved-rule editor**

Add this block below the custom rule checkbox list:

```svelte
				<div class="custom-rule-editor">
					<input
						type="text"
						bind:value={newRuleLabel}
						placeholder="Rule label"
						disabled={scanning}
					/>
					<input
						type="text"
						bind:value={newRulePattern}
						placeholder="Regex, e.g. (^|/)dist$"
						disabled={scanning}
					/>
					<select bind:value={newRuleAction} disabled={scanning}>
						<option value="ignore">Ignore</option>
						<option value="archive">Archive</option>
					</select>
					<button class="secondary" onclick={handleAddCustomRule} disabled={scanning}>
						Add Rule
					</button>
				</div>
```

Add function:

```ts
	async function handleAddCustomRule() {
		if (!newRuleLabel.trim() || !newRulePattern.trim()) return;
		customRulesError = null;
		const nextRules = [
			...customScanRules,
			{
				id: `rule_${Date.now().toString(16)}`,
				label: newRuleLabel.trim(),
				pattern: newRulePattern.trim(),
				action: newRuleAction,
				enabled: true
			}
		];
		try {
			customScanRules = await saveCustomScanRules(nextRules);
			activeCustomRuleIds = customScanRules.filter((rule) => rule.enabled).map((rule) => rule.id);
			newRuleLabel = '';
			newRulePattern = '';
			newRuleAction = 'ignore';
		} catch (e) {
			customRulesError = String(e);
		}
	}

	async function handleRemoveCustomRule(ruleId: string) {
		customRulesError = null;
		const nextRules = customScanRules.filter((rule) => rule.id !== ruleId);
		try {
			customScanRules = await saveCustomScanRules(nextRules);
			activeCustomRuleIds = activeCustomRuleIds.filter((id) => id !== ruleId);
		} catch (e) {
			customRulesError = String(e);
		}
	}
```

Show error after editor:

```svelte
				{#if customRulesError}
					<div class="error">{customRulesError}</div>
				{/if}
```

- [ ] **Step 8: Add CSS**

Near scan dialog CSS, add:

```css
	.scan-rules {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 8px 0;
	}

	.custom-rule-editor {
		display: grid;
		grid-template-columns: 1fr 1fr 96px auto;
		gap: 8px;
		align-items: center;
	}

	.custom-rule-editor select {
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 8px 10px;
		font-size: 13px;
	}
```

- [ ] **Step 9: Run frontend check**

Run:

```bash
npm --prefix app run check
git show --check HEAD
```

Expected: `svelte-check found 0 errors and 0 warnings`.

- [ ] **Step 10: Commit**

```bash
git add app/src/routes/+page.svelte app/src/lib/api/tauri.ts
git commit -m "feat: add scan rule controls"
```

---

### Task 8: Final Verification

**Files:**
- No planned file changes unless verification exposes a defect.

- [ ] **Step 1: Run core tests**

Run:

```bash
cargo test -p dedup-core
```

Expected: all tests pass, including scan rule tests and previous `.git` bundling tests.

- [ ] **Step 2: Run full Rust check**

Run:

```bash
cargo check
```

Expected: pass for `dedup-core`, `dedup-cli`, and Tauri app crate.

- [ ] **Step 3: Run frontend check**

Run:

```bash
npm --prefix app run check
```

Expected: `svelte-check found 0 errors and 0 warnings`.

- [ ] **Step 4: Check CLI help**

Run:

```bash
cargo run -p dedup-cli -- scan --help
```

Expected: help includes:

- `--bundle-git-dirs`
- `--ignore-rust-target`
- `--ignore-node-modules`
- `--ignore-python-venv`
- `--ignore-regex`
- `--archive-regex`

- [ ] **Step 5: Inspect final status and diff**

Run:

```bash
git status --short
git diff --stat HEAD~7..HEAD
git diff --check HEAD~7..HEAD
```

Expected: tracked worktree is clean; diff is limited to files listed in this plan; diff check passes.

- [ ] **Step 6: Commit verification fixes if needed**

If verification required fixes, stage only files from this plan and commit:

```bash
git add crates/dedup-core/Cargo.toml crates/dedup-core/src/types.rs crates/dedup-core/src/lib.rs crates/dedup-core/src/scanner.rs
git add crates/dedup-cli/src/main.rs
git add app/src-tauri/Cargo.toml app/src-tauri/src/workspace.rs app/src-tauri/src/commands.rs app/src-tauri/src/main.rs
git add app/src/lib/api/tauri.ts app/src/routes/+page.svelte
git commit -m "fix: stabilize scan rules"
```

If no fixes were needed, do not create an empty commit.
