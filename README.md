# dedup

Content-addressed file deduplication tool with a desktop visualizer.

`dedup` scans directories, hashes every file with BLAKE3, compresses unique content with LZ4, and stores it in a content-addressed blob store. Duplicate files are detected automatically and stored only once. A Tauri + Svelte 5 desktop app lets you browse the virtual filesystem and inspect duplicates.

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

This creates a `.store/` directory containing compressed blobs and a metadata database. Files with identical content are stored once.

```
Scan complete!
  Files:           1,204
  Directories:     47
  Unique blobs:    983
  Duplicate files: 221
  Original size:   4.2 GB
  Stored size:     2.1 GB
  Space saved:     2.1 GB (50.0%)
```

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
  │  walkdir + BLAKE3 hashing
  ▼
Content Store (.store/)
  ├── blobs/          LZ4-compressed, named by CIDv1
  └── metadata.redb   Virtual path → CID mapping
```

- **Hashing**: BLAKE3 wrapped in CIDv1 (IPFS-compatible identifiers)
- **Compression**: LZ4 frame format (~3 GB/s decompression)
- **Metadata**: redb (pure-Rust, ACID, supports prefix range scans)
- **Dedup index**: CID → \[paths\] multimap for instant duplicate lookup

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
