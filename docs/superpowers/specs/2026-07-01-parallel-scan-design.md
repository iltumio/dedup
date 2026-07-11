# Parallel Scan Pipeline Design

## Goal

Speed up scanning by running the metadata-bound planning phase (directory walk +
change detection) and the CPU/IO-bound execution phase (read + hash + compress +
store) in parallel, while batching metadata commits. The optimization primarily
targets incremental re-backups on SSD/network storage, where change detection has
already collapsed the runtime onto the walk/stat phase. Single-threaded behavior
remains the default and the fallback.

## Requirements

- Parallelism is opt-in via `ScanOptions.parallelism`; the existing single-threaded
  path is used when parallelism is `<= 1` or the tree is small.
- All existing semantics are preserved: scan rules (ignore/archive), git-dir
  bundling, change detection, deletion detection (prune), error logging,
  cooperative cancellation, progress callbacks, and `ScanStats`.
- Content and stored-metadata results are identical to a single-threaded scan
  (byte-for-byte blobs, same CIDs, same DB entries, same `ScanStats` for
  order-insensitive fields).
- Public API signatures and caller code are unchanged; `on_progress` and
  `should_cancel` closures keep their non-`Send` `Fn` bounds.
- No new correctness bug under concurrency (same-CID writes, unique-blob
  accounting, error-log integrity).

## Core Principle: One Writer Thread Owns All Mutable State

Rather than sharing `stats`, `seen`, redb writes, the error log, and progress
across threads with locks, all mutable state lives on a single aggregator/writer
thread. Executor workers are pure functions: bytes in, `(cid, compressed blob)`
out. Consequences:

- `stats` and `seen` are mutated on one thread — no `Mutex`/`DashMap`.
- `on_progress: Fn(&ScanProgress)` stays single-threaded — no new `Send + Sync`
  bound, so the public API is unchanged.
- redb requires a single writer anyway — natural fit.
- Content stays deterministic (dedup is content-addressed; archive tar building
  already sorts internally). Only progress-event and error-log ordering become
  nondeterministic (documented).

## Topology

```
              ignore::WalkBuilder (P walker threads)  -- the planner
                     | rule match + change-detection decision
     +---------------+----------------+
     | work_tx (bounded)              | write_tx (bounded)
     v                                v
  executor pool (M threads)      +----------------------------+
  read -> BLAKE3 -> LZ4 -> store |  aggregator / writer (1)   |
     |  WriteOp::File            |  * owns stats, seen        |
     +-------- write_tx ---------|  * batched redb commits    |
                                 |  * unique-CID accounting   |
                                 |  * error log + on_progress |
                                 +----------------------------+
```

- Walker threads (`ignore` crate, parallel): metadata-bound work — `readdir` +
  `stat`, rule matching, and the change-detection lookup (`get_file` + `exists`;
  redb allows concurrent read txns). Emit lightweight items. On an Ignore/Archive
  directory match, return `WalkState::Skip` so the subtree is not descended; an
  archive subtree is consumed as one unit downstream.
- Executor threads pull `File`/`Archive` items over a bounded channel:
  `fs::read` -> `compute_cid` -> `ContentStore::store`, then emit
  `WriteOp::File`. `Archive` items call the existing self-contained
  `build_directory_archive` (deterministic).
- Writer/aggregator drains `write_tx` until all senders drop; on clean drain it
  does a final commit then `prune_missing(seen)`.
- Dir / Unchanged / Skipped / Error items go straight from walker to writer.

Channel close = clean shutdown: when the walker and all executors drop their
`write_tx` clones, the writer's receive loop ends.

## Item Types (sketch)

```rust
enum PlanItem {
    File { abs: PathBuf, virtual_path: String, size: u64, modified: i64, created: i64, permissions: u32 },
    Archive { abs: PathBuf, virtual_path: String, root_name: String, modified: i64, created: i64 },
    Dir { virtual_path: String, modified: i64 },
    Unchanged { virtual_path: String, original_size: u64 },
    Skipped { virtual_path: String },
    Error { path: String, message: String },
}

enum WriteOp {
    File { virtual_path: String, meta: FileMetadata, cid_str: String, original_size: u64, compressed_size: u64 },
    Dir { virtual_path: String, meta: DirMetadata },
    Unchanged { virtual_path: String, original_size: u64 },
    Skipped { virtual_path: String },
    Error { path: String, message: String },
}
```

Walker emits `PlanItem`; executor converts `File`/`Archive` -> `WriteOp::File`;
everything else flows straight to the writer.

## Required Correctness Fixes (Prerequisites)

These are real bugs the single-threaded code hides; they must land before or with
the pipeline.

1. `ContentStore::store` temp-file race. The temp path is a fixed
   `<cid>.lz4.tmp`. Two workers storing the same CID both create and write the
   same temp file -> corruption. Fix: a unique temp name per write (atomic
   counter + process id), then atomic `rename`. Identical content ⇒
   last-writer-wins is safe.
2. `unique_blobs` / `duplicate_files` accounting. Under concurrency two workers
   can both observe `!exists` for the same new CID and both count it "new". Fix:
   the writer owns a `HashSet<cid_str>` and is authoritative for unique vs
   duplicate. Deterministic counts.
3. Error log becomes writer-owned (workers send `Error` ops) so the file handle
   and line ordering stay coherent.

## Batched redb Commits (the sleeper win)

Independent of threading. `MetadataDb::insert_file` currently opens a write txn
and commits (fsync) per file — one durability fsync per file. The writer thread
accumulates ops and commits in batches (e.g. every ~1024 ops or ~50 ms): one
fsync per batch. New API: `MetadataDb::write_batch(files, dirs)` (one txn, many
inserts). This also benefits the existing single-threaded path if routed through
the same batching, and is safe to land on its own.

Crash-consistency ordering: a blob is always written (and fsynced via rename)
before its metadata insert is committed, so a crash can leave an orphan blob
(harmless) but never a metadata entry pointing at a missing blob.

## Cancellation

`should_cancel: Fn()->bool` may not be `Sync`. A small poller thread evaluates it
and sets a shared `AtomicBool` that all threads read. On cancel: stop descending /
stop pulling work, drain, return `Err("scan cancelled")`, and SKIP prune (prune
only after a fully successful walk — same guarantee as today). In-flight batches
are discarded or committed as already-durable blobs allow; no prune runs.

## Configuration and Fallback

- `ScanOptions.parallelism: Option<usize>` — `None` = auto via
  `std::thread::available_parallelism`, capped; walker gets a modest slice
  (metadata parallelism saturates fast).
- Fall back to the current single-threaded path when `parallelism <= 1` or the
  tree is tiny (e.g. `< 64` entries), avoiding thread spin-up overhead.
- Branch inside the existing public function so no caller signatures change.
- CLI gets `--jobs N`; Tauri wiring later (`serde(default)` keeps it compatible).

## Dependencies

- `ignore` (ripgrep's parallel walker; yields metadata) or `jwalk`.
- `crossbeam-channel` (bounded channels; clean multi-producer close).
- Worker pool: hand-rolled `std::thread` (fits streaming producer/consumer better
  than rayon's scoped `par_iter`). No `num_cpus` dep (`available_parallelism`).

## Rollout (independently shippable)

1. Phase 1 (no threads): `ContentStore` unique-temp fix + `write_batch`
   batching. Benefits the serial path; low risk; land + test alone.
2. Phase 2: pipeline behind `parallelism`, default still serial. Add
   "parallel == serial" equivalence tests across simple / duplicates / ignore /
   archive / git-bundle / rescan-unchanged / prune corpora, plus a same-CID
   stress test and a cancellation test.
3. Phase 3: flip default to auto; expose `--jobs`; wire Tauri.

## Expectations and Risks

- Incremental backups on SSD/network FS: the target case — large win (parallel
  `stat` + batched commits compound).
- First-time backups: win from the executor (parallel hash/compress) + batched
  commits.
- Single HDD: parallel walk can regress -> `--jobs 1` and the small-tree fallback
  exist for this.
- Complexity is real but contained by the one-writer-owns-state rule.
