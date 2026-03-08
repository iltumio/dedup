# dedup — Content-Addressed Storage with Virtual Filesystem

## High-Level Design

```
┌─────────────────────────────────────────────────────┐
│  Source Directory                                    │
│  /home/user/photos/                                 │
│    ├── vacation/img1.jpg                            │
│    ├── vacation/img2.jpg                            │
│    └── backup/img1.jpg  ← duplicate of img1.jpg    │
└──────────────┬──────────────────────────────────────┘
               │ scan (walkdir + blake3)
               ▼
┌─────────────────────────────────────────────────────┐
│  Content Store  (flat dir of compressed blobs)      │
│  .store/blobs/                                      │
│    ├── bafk...a7f3.lz4   ← img1.jpg (stored ONCE)  │
│    └── bafk...b2e1.lz4   ← img2.jpg                │
│                                                     │
│  Metadata DB  (.store/metadata.redb)                │
│  ┌────────────────────────────────────────────────┐ │
│  │ paths table:  path → {cid, size, mtime, type}  │ │
│  │ cid_paths:    cid  → [path1, path2, ...]       │ │
│  │ dirs table:   dir_path → {child_count, mtime}  │ │
│  └────────────────────────────────────────────────┘ │
└──────────────┬──────────────────────────────────────┘
               │ Tauri IPC commands
               ▼
┌─────────────────────────────────────────────────────┐
│  Tauri + Svelte 5 Visualizer                        │
│  Presents virtual directory tree                    │
│  Files decompressed on-demand (LZ4 streaming)       │
│  Duplicate detection panel                          │
└─────────────────────────────────────────────────────┘
```

## Crate Selection

| Component | Crate | Why |
|---|---|---|
| **Hashing** | `blake3` | 3-6x faster than SHA-256, 32-byte digest. Wrap in CIDv1 via `cid` + `multihash_codetable` for IPFS-like identifiers |
| **CID format** | `cid` 0.11 + `multihash_codetable` | Proper IPFS-compatible CIDv1: `Cid::new_v1(RAW, Code::Blake3_256.digest(&data))` |
| **Metadata DB** | `redb` 3.1 | Pure Rust, ACID, prefix `range()` scans for dir listing, `MultimapTableDefinition` for CID→paths dedup index |
| **Compression** | `lz4_flex` 0.12 | 3+ GB/s decompression — a 50MB file decompresses in ~15ms. Frame format supports streaming |
| **Dir walking** | `walkdir` | Standard recursive directory traversal |
| **Serialization** | `bincode` or `postcard` | Compact binary encoding for `FileMetadata` values in redb |
| **Frontend** | Tauri v2 + Svelte 5 | Desktop app with Rust backend commands |

## Workspace Structure

```
dedup/
├── Cargo.toml                    # workspace root
├── crates/
│   ├── dedup-core/               # Library: scanning, hashing, storage, metadata
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── scanner.rs        # walkdir → hash → store pipeline
│   │       ├── content_store.rs  # blob storage (write/read LZ4 compressed)
│   │       ├── metadata.rs       # redb schema + queries
│   │       ├── cid.rs            # CIDv1 generation wrapper
│   │       └── types.rs          # FileMetadata, DirEntry, etc.
│   └── dedup-cli/                # Binary: CLI for scanning directories
│       ├── Cargo.toml
│       └── src/main.rs
├── app/                          # Tauri + Svelte visualizer
│   ├── src-tauri/
│   │   ├── Cargo.toml            # depends on dedup-core
│   │   └── src/
│   │       ├── main.rs
│   │       └── commands.rs       # Tauri IPC commands
│   └── src/                      # Svelte 5 frontend
│       ├── lib/
│       │   ├── components/
│       │   │   ├── FileTree.svelte
│       │   │   ├── TreeNode.svelte
│       │   │   └── FileDetails.svelte
│       │   └── api/
│       │       └── tauri.ts
│       └── routes/
│           └── +page.svelte
```

## Database Schema (redb)

```rust
use redb::{TableDefinition, MultimapTableDefinition};

/// path ("/vacation/img1.jpg") → serialized FileMetadata
const PATHS: TableDefinition<&str, &[u8]> = TableDefinition::new("paths");

/// CID bytes → all paths with that content (dedup index)
const CID_PATHS: MultimapTableDefinition<&[u8], &str> =
    MultimapTableDefinition::new("cid_paths");

/// Directory path → serialized DirMetadata (child count, mtime)
const DIRS: TableDefinition<&str, &[u8]> = TableDefinition::new("dirs");
```

### FileMetadata

```rust
#[derive(Serialize, Deserialize)]
pub struct FileMetadata {
    pub cid: Vec<u8>,          // CIDv1 bytes
    pub original_size: u64,    // uncompressed size
    pub compressed_size: u64,  // on-disk size
    pub modified: i64,         // unix timestamp
    pub created: i64,
    pub permissions: u32,
}
```

### Key Query Patterns

- **List dir** → `PATHS.range("/vacation/"..)` + filter immediate children
- **Resolve file** → `PATHS.get("/vacation/img1.jpg")` → get CID → read blob
- **Find duplicates** → `CID_PATHS.get(cid)` → all paths sharing that content
- **Open file** → CID → `.store/blobs/{cid_hex}.lz4` → LZ4 streaming decompress

## Scan Pipeline

```
walkdir("/source")
  → for each file:
      1. blake3::hash(file_bytes)
      2. cid = Cid::new_v1(RAW, multihash)
      3. if !blob_exists(cid): compress(lz4) → write .store/blobs/{cid}.lz4
      4. insert PATHS[relative_path] = FileMetadata { cid, size, ... }
      5. insert CID_PATHS[cid] → relative_path
  → for each directory:
      6. insert DIRS[relative_path] = DirMetadata { ... }
```

## Tauri Commands

```rust
#[tauri::command]
async fn list_dir(state: State<'_, AppState>, path: String) -> Result<Vec<DirEntry>, String>;

#[tauri::command]
async fn get_file_metadata(state: State<'_, AppState>, path: String) -> Result<FileMetadata, String>;

#[tauri::command]
async fn read_file(state: State<'_, AppState>, path: String) -> Result<Vec<u8>, String>;
// Reads blob by CID, decompresses LZ4, returns bytes

#[tauri::command]
async fn find_duplicates(state: State<'_, AppState>, path: String) -> Result<Vec<String>, String>;
// Given a file path, returns all other paths with same content

#[tauri::command]
async fn scan_directory(state: State<'_, AppState>, source: String) -> Result<ScanStats, String>;
// Trigger a scan from the UI
```

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Hash algorithm | blake3 wrapped in CIDv1 | 3x faster than SHA-256, IPFS-compatible format, future-proof |
| Compression | LZ4 (frame format) | 3GB/s decompression = instant file open in Tauri. ~35% size vs zstd's ~50%, but speed wins for a visualizer |
| Blob naming | `{cid_base32}.lz4` | Human-readable, no collisions, self-describing |
| Metadata DB | redb 3.1 | Pure Rust, ACID, prefix range scans, multimap tables. Actively maintained (sled is abandoned) |
| Path encoding | Forward-slash normalized, relative to scan root | Portable across OS, clean prefix scans |
| Chunking | Whole-file (no chunking) | Simpler. Chunking (FastCDC) only needed for huge files or incremental sync — can add later |

## Prior Art

- **[acid-store](https://github.com/lostatc/acid-store)** — Most similar architecture (virtual FS + CAS + dedup). Unmaintained but excellent reference for the abstraction layer
- **[casq](https://github.com/roobie/casq)** — Minimal CAS with blake3 + zstd. Good reference for simple blob store
- **[rustic](https://github.com/rustic-rs/rustic)** — Production backup tool with CAS. Good reference for scan pipeline
- **[Spacedrive](https://github.com/spacedriveapp/spacedrive)** — Tauri-based virtual file explorer (React, not Svelte, but same concept)

## Implementation Order

1. **`dedup-core`** — scanner, content store, redb metadata, CID wrapper
2. **`dedup-cli`** — CLI to scan a source directory into a `.store/`
3. **`app/`** — Tauri + Svelte visualizer that browses the virtual tree
