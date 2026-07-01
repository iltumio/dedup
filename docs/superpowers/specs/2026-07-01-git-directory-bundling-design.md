# Git Directory Bundling Design

## Context

The scanner currently walks every regular file in the source tree, hashes each file, stores each file as an LZ4-compressed content-addressed blob, and records one virtual file metadata row per file. That behavior is inefficient for Git repositories because `.git` directories contain many small internal files that are not useful to deduplicate independently.

## Requirements

- Add an opt-in per-scan option to bundle Git metadata directories.
- When enabled, any directory whose basename is exactly `.git` is serialized into one deterministic archive payload.
- The archive payload is stored as a single normal blob using the existing content store.
- The virtual filesystem records one file entry at the same location as the original `.git` directory, using the name `.git.tar`.
- The scanner must not walk or record the individual entries inside a bundled `.git` directory.
- The option is disabled by default for backwards compatibility.
- Cancellation must still be honored while scanning and while creating a `.git` bundle.

## User Interface

- CLI: add `dedup scan --bundle-git-dirs`.
- App: add an unchecked checkbox in the scan dialog labeled `Bundle .git directories`.
- Tauri/API: pass the option through with the existing scan request.

## Core Design

Introduce `ScanOptions` in `dedup-core` with `bundle_git_dirs: bool`. Existing scan entry points keep their current behavior by using `ScanOptions::default()`. New option-aware scan entry points route to the same scanner loop with explicit options.

When the scanner sees a directory named `.git` and `bundle_git_dirs` is true, it:

1. Builds a deterministic tar archive of that directory.
2. Sorts archive entries by relative path so repeated scans of the same content produce stable bytes.
3. Normalizes tar metadata that would otherwise vary across machines or runs, including mtime, uid, gid, uname, and gname.
4. Includes regular files and directories; symlinks or special entries are skipped consistently with the current scanner's treatment of special files.
5. Checks cancellation between archived entries.
6. Stores the archive bytes as one blob.
7. Inserts file metadata for `<virtual parent>/.git.tar`.
8. Skips descending into the `.git` directory.

The implementation should use the standard `tar` crate rather than hand-rolling archive bytes.

## Data And Stats

A bundled `.git` directory counts as:

- `total_files += 1`
- `unique_blobs += 1` if the archive blob is new, otherwise `duplicate_files += 1`
- `total_original_bytes += archive_bytes.len()`
- `total_stored_bytes += compressed archive blob size` only when the blob is new

The original `.git` directory itself should not be recorded as a virtual directory when bundled. The visible result is a single file named `.git.tar`.

## Error Handling

If archive creation fails for a `.git` directory, log the error through the existing scan error log path, increment `skipped_files`, and continue scanning siblings. This matches the current scanner behavior for unreadable files and directories.

## Testing

Add focused core tests:

- Default scan still indexes files inside `.git` as individual files.
- Opt-in scan stores `.git.tar` and does not expose `.git` child entries.
- Two identical `.git` directories produce duplicate archive blob accounting.
- Cancellation can stop during `.git` bundling.

Add compile/type checks for CLI, Tauri, and Svelte scan option wiring.
