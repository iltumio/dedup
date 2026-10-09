<p align="center">
  <img src="app/src-tauri/icons/icon.svg" alt="dedup logo" width="112" height="112">
</p>

<h1 align="center">dedup</h1>

<p align="center">
  Content-addressed file archiving with FastCDC deduplication, LZ4 compression, a Rust CLI, and a desktop app.
</p>

<p align="center">
  <a href="https://github.com/iltumio/dedup/releases/latest"><img src="https://img.shields.io/github/v/release/iltumio/dedup?label=version" alt="Latest release"></a>
  <a href="https://github.com/iltumio/dedup/actions/workflows/release.yml"><img src="https://img.shields.io/github/actions/workflow/status/iltumio/dedup/release.yml?label=build" alt="Release build status"></a>
</p>

`dedup` archives files while storing identical content only once. FastCDC shares
unchanged parts of similar files, BLAKE3 identifies and verifies content, and LZ4
compresses the stored data. Use the CLI or the Tauri + Svelte desktop app to add
files, browse the archive, inspect duplicates, and extract content. Original
whole-file archives remain supported, with resumable migration to FastCDC.

## Install

### Quick install (CLI or App)

The install script will ask whether you want the CLI or the desktop app:

```sh
curl -fsSL https://raw.githubusercontent.com/iltumio/dedup/main/install.sh | sh
```

### From source

Requires [Rust](https://rustup.rs/) and [just](https://github.com/casey/just).

```sh
just build-cli   # CLI binary → target/release/dedup
just build-app   # Desktop app (also needs Node.js 22+)
```

## Usage

### Scan a directory

```sh
dedup scan --source ~/photos
```

This creates a `.store/` directory containing compressed chunks, a manifest database, and filesystem metadata. New stores use format v3 with FastCDC and digest-based blob subdirectories; existing v1 whole-file and v2 flat FastCDC archives remain supported.

In the desktop app, **Create an archive** includes an **Archive format** choice:
**FastCDC** (default) shares identical files and chunks of similar files;
**Original (whole files)** shares identical whole files. Choose a new or empty
folder. The selected format is initialized immediately and preserved when the
archive is reopened. Existing archives opened through **Open existing** keep
their format; original archives can later be migrated to FastCDC.

```
Scan complete!
  Files:           1,204
  Directories:     47
  Unique files:    983
  Duplicate files: 221
  Original size:   4.2 GB
  Stored size:     2.1 GB
  Space saved:     2.1 GB (50.0%)
```

### Migrate an existing archive to FastCDC

In the desktop app, open **Manage archives**, expand **Archive options** on the
source archive, and choose **Migrate to FastCDC**. Choose a name and a separate
destination folder and the number of **Parallel files** (default 4), then start
migration. The dialog shows elapsed time, processed bytes, read and verification
rate, and each active file's conversion or verification progress. Updates arrive
every half second, including while preparing the index and calculating final
totals. A separate message reports when data has stopped advancing, even if the
app still responds. The dialog offers **Stop migration**. Interrupted jobs retain their destination in the
archive list: choose **Resume migration**, even after restarting the app.
On completion the new archive is added automatically; choose **Open migrated
archive** to switch to it, or keep the original open. Scans and archive switching
are blocked while conversion runs.

The same conversion is also available from the CLI:

```sh
dedup migrate --source .store --destination .store-v2 --workers 4
```

Migration reconstructs content directly from the archived LZ4 blobs; the original
source files are not needed. It converts each distinct file CID once, verifies the
reconstructed destination with BLAKE3, and preserves paths, exact-file CIDs,
directory metadata, dates, permissions, and duplicate groups. The original archive
is opened without committing any writes and remains available for rollback.

The destination must be empty or belong to the same migration. A dedicated
subdirectory is supported when the source archive occupies a disk root, e.g.
`/run/media/user/T7/fastcdc`. The original archive's `blobs` directory and internal
database/checkpoint paths are protected; the destination cannot be the source
itself or one of its ancestors. Symlink aliases are checked as well. If interrupted,
run the same command again: verified chunks and manifests are reused, damaged
checkpoints are repaired from the source, and metadata commits are resumed.
A changed source archive or chunking profile is rejected. Incomplete destinations
cannot be opened as normal stores. Keep the source unchanged until completion.
Both archives coexist, so allow space for the new archive during conversion.
Checkpoint and chunk publication use durable temporary files, file locks and
atomic renames; hard links are not required, including on exFAT volumes.

File workers handle reading, chunking and disk publication. Chunk BLAKE3 hashing
and LZ4 compression run on a shared Rayon CPU pool, including for a single large
file. The CPU pool defaults to up to four available CPUs; buffers are batched at
2 MiB per file (or one maximum-size chunk for larger custom profiles), with
additional compression and FastCDC buffers. On Linux 5.8 or newer, a filesystem durability
barrier synchronizes chunk data and directory metadata before the file manifest
commits, avoiding a separate disk flush per chunk. This also flushes other pending
writes on that filesystem. Other kernels and platforms retain per-blob data synchronization
and synchronize the touched directory hierarchy before committing the manifest.
`--workers` accepts 1–32 and can change when resuming an existing checkpoint,
including checkpoints created by the serial migrator. Concurrent disk reads and
writes may limit the benefit on slower drives. The displayed processing rate
counts uncompressed reads and verification, rather than physical disk writes.

Final archive totals collect referenced chunks once and read their sizes in native
directory order, including mixed flat/sharded archives. Shared chunks contribute
once; unreferenced historical blobs are excluded. The desktop app reports separate
manifest-reading and chunk-counting progress during this stage, which can be stopped
and resumed without discarding converted content.

After completion, use `--store .store-v2` or import that directory into the desktop
app. Releases that only support v1/v2 cannot open the v3 layout; use the original
archive to return to an older release. Resuming an old v2 migration destination
enables v3 writes immediately: existing flat chunks remain readable and new
chunks go into subdirectories. Original source archives remain unchanged.

To reorganize an existing FastCDC archive completely, close it in the app and run:

```sh
dedup optimize --store /path/to/fastcdc-archive
```

This upgrades the destination in place, preserving content CIDs, chunk boundaries,
manifests and filesystem metadata. Run it again after interruption to resume.
Each flat blob is copied and synchronized before its old name is removed; allow
space for the largest compressed blob. Mixed flat/sharded layouts remain readable.
The layout uses `blobs/ab/c/<CID>.lz4`, with `ab/c` derived from the first 12 digest
bits, distributing content across up to 4,096 leaf directories. Legacy whole-file
archives must be migrated to FastCDC before optimization.

The default persisted profile is FastCDC v2020, normalization level 1, seed 0,
with minimum/target/maximum sizes of 16/64/256 KiB. Migration also accepts
`--min-chunk-size`, `--avg-chunk-size`, and `--max-chunk-size` in bytes. Target sizes
must be powers of two; reruns must use the original migration profile.

### Browse the virtual filesystem

```sh
dedup ls /
dedup ls /photos/vacation
```

### Inspect a file

```sh
dedup info /photos/vacation/img1.jpg
```

Shows CID, sizes, timestamps, and any duplicate copies.

### Find all duplicates

```sh
dedup duplicates
```

### Extract a file

```sh
dedup cat /photos/vacation/img1.jpg -o restored.jpg
```

## How it works

```
Source directory
  │
  │  walkdir + BLAKE3 hashing + FastCDC
  ▼
Content Store (.store/)
  ├── blobs/ab/c/     LZ4-compressed chunks, named by CIDv1 (v3)
  ├── content.redb    Format/profile + file CID → ordered chunk manifest
  └── metadata.redb   Virtual paths, dates, permissions, duplicate index
```

- **Hashing**: whole-file and chunk BLAKE3 wrapped in CIDv1; file identity is independent of chunk boundaries
- **Chunking**: FastCDC v2020 with a versioned, persisted profile; chunk boundaries are computed before compression
- **Compression**: LZ4 frame format (~3 GB/s decompression)
- **Metadata**: redb (pure-Rust, ACID, supports prefix range scans)
- **Dedup index**: file CID → \[paths\] multimap for exact-file duplicate lookup; chunk sharing is tracked separately

Storage totals count each referenced compressed chunk and each serialized manifest
once, excluding database page allocation, temporary files, and unreferenced
historical content. Per-extension totals split shared payloads equally between the
extensions referencing them, with deterministic rounding. `unique_blobs` in the
library response continues to mean unique whole-file CIDs, not chunk count.
Scan stored-byte totals describe newly published payloads during that scan.

### Compare chunking profiles

```sh
cargo run -p dedup-core --release --example fastcdc_benchmark
cargo run -p dedup-core --release --example fastcdc_benchmark -- /path/to/data
DEDUP_BENCH_THREADS=8 DEDUP_BENCH_PROFILE=64K DEDUP_BENCH_FS_ROOT=/path/to/disk \
  cargo run -p dedup-core --release --example fastcdc_benchmark
```

The first command uses deterministic synthetic file versions; the second reads the
specified directory into memory. Both compare whole-file storage with FastCDC
64 KiB and 256 KiB profiles, measuring write/verified-read throughput, compressed
blob bytes, actual manifest database size, and blob count. Temporary benchmark
stores are removed automatically. `DEDUP_BENCH_THREADS` selects CPU concurrency,
`DEDUP_BENCH_PROFILE` filters profile names, and `DEDUP_BENCH_FS_ROOT` places
temporary stores on the filesystem being measured. Measure representative data before choosing a
custom profile; smaller chunks can improve sharing while increasing I/O and
metadata costs. Normal scanning currently reads each file into memory; migration
and `ContentStore::copy_to` stream content with bounded chunk buffers.

## Project structure

```
dedup/
├── crates/
│   ├── dedup-core/    # Library: scanning, hashing, storage, metadata
│   └── dedup-cli/     # CLI binary
├── app/               # Tauri v2 + Svelte 5 desktop app
│   ├── src-tauri/
│   └── src/
├── install.sh         # Interactive installer (CLI or App)
└── justfile           # Development commands
```

## Development

Requires [Rust](https://rustup.rs/), [Node.js](https://nodejs.org/) 22+, and [just](https://github.com/casey/just).

```sh
just             # List all commands
just dev         # Run Tauri dev server
just test        # Run all tests
just lint        # Clippy lints
just ci          # Full CI check (fmt + lint + test)
```

## License

[MIT](LICENSE)
