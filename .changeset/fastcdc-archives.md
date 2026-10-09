---
default: minor
---

# FastCDC archives and resumable migration

Add FastCDC chunk deduplication with LZ4 compression to share unchanged content between similar files while preserving whole-file content identifiers. Existing original archives remain readable and writable, and the desktop app now lets users choose FastCDC or the original whole-file format when creating an archive.

Migrate original archives from the desktop app or the CLI into a separate destination with verified, resumable checkpoints. Migration supports configurable parallel file processing, including changing the worker count when resuming, and works on exFAT drives and in dedicated subfolders when the original archive occupies a disk root.

Show elapsed time, processed bytes, read and verification rate, active files and per-file progress during migration. Periodic updates distinguish app responsiveness from stalled data progress. Count shared chunks once in archive and extension statistics.
