# Scan Rules Design

## Goal

Add an extensible scan-rule system so a scan can either skip matched paths or store matched directories as one archive blob. This generalizes the current `.git` bundling behavior and adds opt-in presets for common project artifacts such as Rust `target`, Node `node_modules`, and Python virtual environments.

## Requirements

- Built-in presets are opt-in per scan.
- Custom regex rules are saved at app level and reused across scans.
- A rule has a regex pattern and one action: `ignore` or `archive`.
- Regexes match the full scan-relative path string using `/` separators.
- If a rule matches, the scanner applies the rule and does not evaluate later rules for that path.
- Invalid regexes fail before scanning starts with a clear error.
- Existing scan behavior remains unchanged when no rules are enabled.
- Existing `.git` bundling behavior remains available, but internally becomes the `.git` archive preset.

## Rule Model

Core adds:

```rust
pub enum ScanRuleAction {
    Ignore,
    Archive,
}

pub struct ScanRule {
    pub pattern: String,
    pub action: ScanRuleAction,
}
```

`ScanOptions` changes from a single-purpose `.git` option to rule-based options:

```rust
pub struct ScanOptions {
    pub bundle_git_dirs: bool,
    pub rules: Vec<ScanRule>,
}
```

`bundle_git_dirs` is kept for compatibility and is translated into the `.git` archive preset when building the effective rules. New call sites should prefer `rules`.

## Matching Semantics

The scanner computes the scan-relative path string before normal file or directory handling. Examples:

- `.git`
- `target`
- `apps/web/node_modules`
- `apps/web/node_modules/react/index.js`
- `services/api/.venv`

Each rule regex runs against that string. First match wins.

The effective rule list is ordered. Core respects the order it receives in `ScanOptions.rules`. The UI builds that list in a stable order: selected built-in presets first, then enabled custom rules in their saved order. The initial implementation does not need custom drag-and-drop rule reordering.

Rule actions:

- `ignore`: skip a matched file; for a matched directory, call `skip_current_dir()` and skip all children.
- `archive`: for a matched directory, store it as `<virtual_path>.tar` and skip all children; for a matched regular file, skip it.

Archive rules use the same deterministic tar behavior as the current `.git` bundling code: sorted archive entries, normalized tar metadata, symlink/special-file skipping, cancellation propagation, duplicate accounting, and progress reporting.

## Built-In Presets

Built-in presets are represented as ordinary rules:

| Preset | Pattern | Action |
| --- | --- | --- |
| `.git` directories | `(^|/)\.git$` | `archive` |
| Rust build output | `(^|/)target$` | `ignore` |
| Node dependencies | `(^|/)node_modules$` | `ignore` |
| Python virtual envs | `(^|/)(\.venv|venv)$` | `ignore` |

The UI may group these as checkboxes, but the core scanner only sees rules.

## App-Level Custom Rules

The existing app config (`WorkspacesConfig`) gains an app-level list of custom rules. These rules are not tied to a workspace.

Custom rule fields:

- `id`: stable UI identifier.
- `label`: user-facing name.
- `pattern`: regex string applied to full scan-relative paths.
- `action`: `ignore` or `archive`.
- `enabled`: default selection for new scan dialogs.

The scan dialog loads saved custom rules and includes enabled ones in the scan request. Users can edit the saved list from the app UI. Editing a custom rule affects later scans, not an already running scan.

## CLI And Tauri Surface

CLI should expose the presets and custom ad hoc rules:

- `--bundle-git-dirs` remains as compatibility alias for the `.git` archive preset.
- New preset flags can include `--ignore-rust-target`, `--ignore-node-modules`, and `--ignore-python-venv`.
- Generic repeated flags can include `--ignore-regex <PATTERN>` and `--archive-regex <PATTERN>`.

Tauri scan requests should accept explicit rules for that scan. The UI builds those rules from selected presets and saved app-level custom rules.

The existing optional `bundle_git_dirs` Tauri parameter remains supported and maps to the `.git` archive preset for compatibility.

## UI Behavior

The scan dialog shows built-in preset checkboxes. All built-in presets default off for each scan.

Saved custom rules are reused across scans through app config. Each custom rule has its own saved default enabled state. The scan dialog starts from those saved defaults and lets the user toggle rules for the current scan without changing the saved default unless they edit the rule.

The `.git` preset replaces the previous single-purpose “Bundle .git directories” behavior. The label should make the archive action explicit, for example `Archive .git directories`.

## Error Handling

- Invalid regexes in scan request return an error before any filesystem entries are stored.
- Invalid saved custom rules are rejected when saving them.
- Archive failures that are not cancellation are logged and counted as skipped, matching current `.git` behavior.
- Cancellation returns `scan cancelled` and is not logged as a skipped file.

## Testing

Core tests:

- Default scan with no rules indexes files and directories exactly as before.
- Ignore rule skips a matched directory and all children.
- Ignore rule skips a matched file.
- Archive rule stores a matched directory as `<path>.tar` and skips child metadata.
- Archive rule matching a regular file skips it.
- Rule order is first-match-wins.
- Invalid regex fails before scanning stores metadata.
- `.git` preset produces the same behavior as the current `.git` bundling option.
- Built-in `target`, `node_modules`, and `.venv` presets ignore nested matching directories.

CLI/Tauri/UI tests or checks:

- CLI help exposes preset and regex flags.
- Tauri accepts rules and still accepts missing/false `bundle_git_dirs`.
- Svelte check passes after rule-management UI changes.

## Non-Goals

- No automatic project-type detection in this pass.
- No `.gitignore` parsing in this pass.
- No per-workspace custom rule storage in this pass.
- No extraction/restoration changes for archived directories in this pass; archived directories remain normal `.tar` files in the virtual filesystem.
