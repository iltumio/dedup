use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use cid::Cid;
use fastcdc::v2020::{Normalization, StreamCDC};
use lz4_flex::frame::{FrameDecoder, FrameEncoder};
use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::blob_store::BlobStore;
use crate::cid as cid_util;

const CHUNK_BATCH_BYTES: usize = 2 * 1024 * 1024;
const CONFIG: TableDefinition<&str, &[u8]> = TableDefinition::new("config");
const MANIFESTS: TableDefinition<&str, &[u8]> = TableDefinition::new("manifests");

/// Persisted FastCDC profile. Variant, gear tables, normalization and seed are
/// fixed by profile version 1 (fastcdc 5.0.0, v2020, Level1, seed 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkingProfile {
    pub version: u32,
    pub min_size: u32,
    pub avg_size: u32,
    pub max_size: u32,
}

impl Default for ChunkingProfile {
    fn default() -> Self {
        Self {
            version: 1,
            min_size: 16 * 1024,
            avg_size: 64 * 1024,
            max_size: 256 * 1024,
        }
    }
}

impl ChunkingProfile {
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.version == 1,
            "unsupported chunking profile version: {}",
            self.version
        );
        ensure!(
            (64..=1_048_576).contains(&self.min_size)
                && (256..=4_194_304).contains(&self.avg_size)
                && (1024..=16_777_216).contains(&self.max_size)
                && self.min_size <= self.avg_size
                && self.avg_size <= self.max_size,
            "invalid FastCDC chunk sizes"
        );
        ensure!(
            self.avg_size.is_power_of_two(),
            "average chunk size must be a power of two"
        );
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct FormatConfig {
    version: u32,
    profile: ChunkingProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChunkRef {
    cid: Vec<u8>,
    size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    size: u64,
    chunks: Vec<ChunkRef>,
}

/// Logical file size on disk and actual newly published storage payload.
pub(crate) struct StoredContent {
    pub compressed_size: u64,
    pub new_bytes: u64,
    pub new_file: bool,
}

/// CID-addressed content storage. Legacy stores use whole-file LZ4 blobs;
/// v2/v3 stores use LZ4 chunks and a separate redb manifest table, preserving the
/// whole-file CID and the legacy FileMetadata encoding.
pub struct ContentStore {
    blobs: BlobStore,
    layout_version: u32,
    chunk_pool: Option<ThreadPool>,
    manifests: Option<Database>,
    profile: Option<ChunkingProfile>,
}

impl ContentStore {
    /// New stores use FastCDC; existing legacy stores retain their format.
    pub fn open(root: &Path) -> Result<Self> {
        Self::open_impl(root, None)
    }

    /// Create a sharded v3 store, or reopen one with exactly the requested profile.
    pub fn open_with_profile(root: &Path, profile: ChunkingProfile) -> Result<Self> {
        profile.validate()?;
        Self::open_impl(root, Some(profile))
    }

    fn open_impl(root: &Path, requested: Option<ChunkingProfile>) -> Result<Self> {
        let db_path = root.join("content.redb");
        let blobs_dir = root.join("blobs");
        let legacy =
            !db_path.exists() && (root.join("metadata.redb").exists() || blobs_dir.exists());
        if legacy {
            ensure!(
                requested.is_none(),
                "legacy store must be migrated to a new directory"
            );
            fs::create_dir_all(&blobs_dir)?;
            return Ok(Self {
                blobs: BlobStore::open(blobs_dir, false, false)?,
                layout_version: 1,
                chunk_pool: None,
                manifests: None,
                profile: None,
            });
        }
        fs::create_dir_all(root)?;
        let existing = db_path.exists();
        let db = if existing {
            Database::open(&db_path)?
        } else {
            Database::create(&db_path)?
        };
        // An interrupted first migration can leave an empty, valid redb file
        // before the format transaction. Only recover that narrowly defined case.
        let uninitialized = if existing {
            let txn = db.begin_read()?;
            matches!(
                txn.open_table(CONFIG),
                Err(redb::TableError::TableDoesNotExist(_))
            )
        } else {
            false
        };
        let recover_initialization = uninitialized
            && requested.is_some()
            && root.join("migration.bin").is_file()
            && !root.join("metadata.redb").exists()
            && !blobs_dir.exists();
        let (layout_version, profile) = if existing && !recover_initialization {
            let txn = db.begin_read()?;
            let config = txn
                .open_table(CONFIG)
                .context("missing v2 format configuration")?;
            let value = config.get("format")?.context("missing v2 format version")?;
            let config: FormatConfig = bincode::deserialize(value.value())?;
            ensure!(
                matches!(config.version, 2 | 3),
                "unsupported store format version: {}",
                config.version
            );
            config.profile.validate()?;
            if let Some(requested) = requested {
                ensure!(
                    requested == config.profile,
                    "store chunking profile differs from requested profile"
                );
            }
            (config.version, config.profile)
        } else {
            let profile = requested.unwrap_or_default();
            let encoded = bincode::serialize(&FormatConfig {
                version: 3,
                profile,
            })?;
            let txn = db.begin_write()?;
            {
                txn.open_table(CONFIG)?
                    .insert("format", encoded.as_slice())?;
                txn.open_table(MANIFESTS)?;
            }
            txn.commit()?;
            sync_dir(root)?;
            (3, profile)
        };
        fs::create_dir_all(&blobs_dir)?;
        let blobs = BlobStore::open(blobs_dir, layout_version == 3, true)?;
        sync_dir(root)?;
        Ok(Self {
            blobs,
            layout_version,
            chunk_pool: Some(Self::chunk_pool(default_chunk_threads())?),
            manifests: Some(db),
            profile: Some(profile),
        })
    }

    pub fn format_version(&self) -> u32 {
        self.layout_version
    }

    fn chunk_pool(threads: usize) -> Result<ThreadPool> {
        ensure!(
            (1..=64).contains(&threads),
            "chunk threads must be between 1 and 64"
        );
        ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|index| format!("chunk-cpu-{index}"))
            .build()
            .context("failed to build chunk CPU pool")
    }

    /// CPU concurrency is independent of the number of files reading/writing.
    /// A single large file can use the entire pool; queues remain batch bounded.
    pub fn set_chunk_parallelism(&mut self, threads: usize) -> Result<()> {
        ensure!(self.profile.is_some(), "chunk parallelism requires FastCDC");
        self.chunk_pool = Some(Self::chunk_pool(threads)?);
        Ok(())
    }

    /// Upgrade an existing flat FastCDC layout without changing CIDs or chunks.
    /// Interrupted upgrades can be resumed; legacy archives require migration.
    pub fn upgrade_blob_layout(&mut self) -> Result<u64> {
        self.upgrade_blob_layout_with_cancellation(|| false)
    }

    pub(crate) fn upgrade_blob_layout_with_cancellation(
        &mut self,
        should_cancel: impl Fn() -> bool,
    ) -> Result<u64> {
        self.enable_sharded_writes()?;
        self.blobs.upgrade(should_cancel)
    }

    // Resume starts writing into small shards immediately. Existing flat blobs
    // stay indexed and readable; the explicit optimizer relocates them later.
    pub(crate) fn enable_sharded_writes(&mut self) -> Result<()> {
        ensure!(
            self.profile.is_some(),
            "legacy archives require migration to FastCDC"
        );
        if self.layout_version == 2 {
            let encoded = bincode::serialize(&FormatConfig {
                version: 3,
                profile: self.profile.unwrap(),
            })?;
            let txn = self.manifests.as_ref().unwrap().begin_write()?;
            txn.open_table(CONFIG)?
                .insert("format", encoded.as_slice())?;
            txn.commit()?;
            self.layout_version = 3;
        }
        self.blobs.enable_sharding();
        Ok(())
    }

    pub fn chunking_profile(&self) -> Option<ChunkingProfile> {
        self.profile
    }

    fn blob_path(&self, cid: &Cid) -> PathBuf {
        self.blobs.path(cid)
    }

    fn manifest(&self, cid: &Cid) -> Result<Option<Manifest>> {
        let Some(db) = &self.manifests else {
            return Ok(None);
        };
        let txn = db.begin_read()?;
        let table = txn.open_table(MANIFESTS)?;
        let key = cid_util::cid_to_string(cid);
        let Some(value) = table.get(key.as_str())? else {
            return Ok(None);
        };
        let manifest: Manifest =
            bincode::deserialize(value.value()).context("invalid file manifest")?;
        ensure!(
            manifest.version == 1,
            "unsupported manifest version: {}",
            manifest.version
        );
        let size = manifest
            .chunks
            .iter()
            .try_fold(0u64, |sum, chunk| sum.checked_add(chunk.size))
            .context("manifest size overflow")?;
        ensure!(
            size == manifest.size && manifest.chunks.iter().all(|c| c.size > 0),
            "invalid manifest lengths"
        );
        Ok(Some(manifest))
    }

    /// Checks that the manifest and every referenced chunk are present.
    pub fn exists(&self, cid: &Cid) -> bool {
        match self.manifest(cid) {
            Ok(Some(manifest)) => manifest.chunks.iter().all(|chunk| {
                cid_util::cid_from_bytes(&chunk.cid).is_ok_and(|cid| self.blob_path(&cid).is_file())
            }),
            Ok(None) => self.blob_path(cid).is_file(),
            Err(_) => false,
        }
    }

    /// Store bytes under their whole-file CID, returning their logical stored size.
    pub fn store(&self, cid: &Cid, data: &[u8]) -> Result<u64> {
        Ok(self.store_detailed(cid, data)?.compressed_size)
    }

    pub(crate) fn store_detailed(&self, cid: &Cid, data: &[u8]) -> Result<StoredContent> {
        self.store_reader(cid, data)
    }

    /// Streaming ingestion used by migration. Chunks become durable before the
    /// manifest is committed; interrupted writes can leave only unreferenced chunks.
    pub(crate) fn store_reader(&self, cid: &Cid, mut reader: impl Read) -> Result<StoredContent> {
        let Some(profile) = self.profile else {
            let mut data = Vec::new();
            reader.read_to_end(&mut data)?;
            ensure!(cid_util::compute_cid(&data) == *cid, "content CID mismatch");
            let (size, created) = self.store_blob(cid, &data)?;
            return Ok(StoredContent {
                compressed_size: size,
                new_bytes: if created { size } else { 0 },
                new_file: created,
            });
        };
        let was_complete = self.manifest(cid).ok().flatten().is_some() && self.exists(cid);
        let mut manifest = Manifest {
            version: 1,
            size: 0,
            chunks: Vec::new(),
        };
        let mut hasher = blake3::Hasher::new();
        let mut new_bytes = 0;
        let mut sizes = BTreeMap::new();
        let mut directories = HashSet::new();
        let mut stream = StreamCDC::with_level(
            reader,
            profile.min_size as usize,
            profile.avg_size as usize,
            profile.max_size as usize,
            Normalization::Level1,
        );
        loop {
            let mut batch = Vec::new();
            let mut batch_bytes = 0;
            let mut read_error = None;
            while batch_bytes < CHUNK_BATCH_BYTES {
                match stream.next() {
                    Some(Ok(chunk)) => {
                        hasher.update(&chunk.data);
                        batch_bytes += chunk.data.len();
                        batch.push(chunk.data);
                    }
                    Some(Err(error)) => {
                        read_error = Some(error);
                        break;
                    }
                    None => break,
                }
            }
            if batch.is_empty() {
                if let Some(error) = read_error {
                    return Err(error).context("failed to read chunk source");
                }
                break;
            }
            // Indexed parallel collection preserves the original chunk order.
            // Only hashing/compression run on Rayon; disk I/O stays on the caller.
            let prepared = if batch.len() == 1 {
                batch
                    .into_iter()
                    .map(prepare_chunk)
                    .collect::<Result<Vec<_>>>()
            } else if rayon::current_thread_index().is_some() {
                // A scan already runs on Rayon. Reuse that pool: waiting on a
                // different pool lets the caller recursively execute other file
                // jobs and can exhaust its stack on large scans.
                batch
                    .into_par_iter()
                    .map(prepare_chunk)
                    .collect::<Result<Vec<_>>>()
            } else {
                self.chunk_pool.as_ref().unwrap().install(|| {
                    batch
                        .into_par_iter()
                        .map(prepare_chunk)
                        .collect::<Result<Vec<_>>>()
                })
            }?;
            for (chunk_cid, original_size, encoded) in prepared {
                let (size, created) = self.blobs.publish(&chunk_cid, &encoded, false)?;
                if created {
                    new_bytes += size;
                }
                directories.insert(self.blob_path(&chunk_cid).parent().unwrap().to_path_buf());
                sizes.insert(chunk_cid, size);
                manifest.size += original_size;
                manifest.chunks.push(ChunkRef {
                    cid: chunk_cid.to_bytes(),
                    size: original_size,
                });
            }
            // Keep already processed chunks reusable if cancellation/read failure
            // happened while filling a batch. Never commit a partial manifest.
            if let Some(error) = read_error {
                return Err(error).context("failed to read chunk source");
            }
        }
        verify_hash(cid, &hasher)?;
        self.blobs.sync_directories(directories)?;
        let encoded = bincode::serialize(&manifest)?;
        let key = cid_util::cid_to_string(cid);
        let txn = self.manifests.as_ref().unwrap().begin_write()?;
        let new_manifest;
        {
            let mut table = txn.open_table(MANIFESTS)?;
            new_manifest = table.get(key.as_str())?.is_none();
            table.insert(key.as_str(), encoded.as_slice())?;
        }
        txn.commit()?;
        if new_manifest {
            new_bytes += encoded.len() as u64;
        }
        let compressed_size = sizes.values().sum::<u64>() + encoded.len() as u64;
        Ok(StoredContent {
            compressed_size,
            new_bytes,
            new_file: !was_complete,
        })
    }

    fn store_blob(&self, cid: &Cid, data: &[u8]) -> Result<(u64, bool)> {
        self.blobs.store(cid, data)
    }

    fn read_blob(&self, cid: &Cid) -> Result<Vec<u8>> {
        self.blobs.read(cid)
    }

    fn blob_reader(&self, cid: &Cid) -> Result<FrameDecoder<fs::File>> {
        self.blobs.reader(cid)
    }

    pub(crate) fn migration_reader(&self, cid: &Cid) -> Result<impl Read> {
        ensure!(
            self.format_version() == 1,
            "migration requires a legacy source store"
        );
        self.blob_reader(cid)
    }

    /// Reconstruct and verify content, supporting both whole-file and chunked stores.
    pub fn read(&self, cid: &Cid) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        self.copy_to(cid, &mut data)?;
        Ok(data)
    }

    /// Streaming restore. Callers must discard output if verification fails.
    pub fn copy_to(&self, cid: &Cid, mut output: impl Write) -> Result<u64> {
        let mut hasher = blake3::Hasher::new();
        let mut total = 0;
        if let Some(manifest) = self.manifest(cid)? {
            for chunk in &manifest.chunks {
                let data = self.read_blob(&cid_util::cid_from_bytes(&chunk.cid)?)?;
                ensure!(data.len() as u64 == chunk.size, "chunk length mismatch");
                hasher.update(&data);
                output.write_all(&data)?;
                total += data.len() as u64;
            }
            ensure!(total == manifest.size, "file length mismatch");
        } else {
            let mut decoder = self.blob_reader(cid)?;
            let mut buffer = [0; 64 * 1024];
            loop {
                let n = decoder.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
                output.write_all(&buffer[..n])?;
                total += n as u64;
            }
        }
        verify_hash(cid, &hasher)?;
        Ok(total)
    }

    pub fn compressed_size(&self, cid: &Cid) -> Result<Option<u64>> {
        if !self.exists(cid) {
            return Ok(None);
        }
        Ok(Some(self.storage_entries(cid)?.values().sum()))
    }

    /// Distinct compressed payloads plus serialized manifest bytes. Database
    /// allocation overhead and unreferenced historical content are excluded.
    pub(crate) fn storage_entries(&self, cid: &Cid) -> Result<BTreeMap<String, u64>> {
        let mut entries = BTreeMap::new();
        if let Some(manifest) = self.manifest(cid)? {
            entries.insert(
                format!("manifest:{cid}"),
                bincode::serialized_size(&manifest)?,
            );
            for chunk in manifest.chunks {
                let cid = cid_util::cid_from_bytes(&chunk.cid)?;
                entries.insert(
                    format!("blob:{cid}"),
                    fs::metadata(self.blob_path(&cid))?.len(),
                );
            }
        } else {
            entries.insert(
                format!("blob:{cid}"),
                fs::metadata(self.blob_path(cid))?.len(),
            );
        }
        Ok(entries)
    }

    pub(crate) fn referenced_size(&self, cids: impl IntoIterator<Item = Cid>) -> Result<u64> {
        let mut seen_files = HashSet::new();
        let mut entries = BTreeMap::new();
        for cid in cids {
            if seen_files.insert(cid) {
                entries.extend(self.storage_entries(&cid)?);
            }
        }
        Ok(entries.values().sum())
    }
}

fn prepare_chunk(data: Vec<u8>) -> Result<(Cid, u64, Vec<u8>)> {
    let cid = cid_util::compute_cid(&data);
    let mut encoder = FrameEncoder::new(Vec::new());
    encoder.write_all(&data)?;
    Ok((cid, data.len() as u64, encoder.finish()?))
}

fn default_chunk_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get().min(4))
}

fn verify_hash(cid: &Cid, hasher: &blake3::Hasher) -> Result<()> {
    let hash = cid::multihash::Multihash::<64>::wrap(0x1e, hasher.finalize().as_bytes())?;
    if Cid::new_v1(0x55, hash) != *cid {
        bail!("content CID mismatch: {cid}");
    }
    Ok(())
}

pub(crate) fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Keep a stable lock file: removing it would let concurrent writers lock
/// different inodes. Each call opens a separate handle, including across threads.
pub(crate) fn lock_file(path: &Path) -> Result<fs::File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("cannot open publication lock {}", path.display()))?;
    file.lock()
        .with_context(|| format!("cannot lock {}", path.display()))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cid::compute_cid;
    use tempfile::TempDir;

    fn payload(size: usize) -> Vec<u8> {
        let mut seed = 0x123456789abcdefu64;
        (0..size)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed as u8
            })
            .collect()
    }

    fn make_flat_v2(root: &Path, bytes: &[u8]) -> Cid {
        let store = ContentStore::open(root).unwrap();
        let cid = compute_cid(bytes);
        store.store(&cid, bytes).unwrap();
        for entry in walkdir::WalkDir::new(root.join("blobs")).follow_links(false) {
            let entry = entry.unwrap();
            if entry.path().extension().is_some_and(|e| e == "lz4") {
                fs::rename(entry.path(), root.join("blobs").join(entry.file_name())).unwrap();
            }
        }
        let txn = store.manifests.as_ref().unwrap().begin_write().unwrap();
        let config = bincode::serialize(&FormatConfig {
            version: 2,
            profile: ChunkingProfile::default(),
        })
        .unwrap();
        txn.open_table(CONFIG)
            .unwrap()
            .insert("format", config.as_slice())
            .unwrap();
        txn.commit().unwrap();
        cid
    }

    #[test]
    fn flat_v2_upgrade_is_readable_after_cancellation_and_resume() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = TempDir::new().unwrap();
        let bytes = payload(4 * 1024 * 1024);
        let cid = make_flat_v2(root.path(), &bytes);
        let mut store = ContentStore::open(root.path()).unwrap();
        assert_eq!(store.format_version(), 2);
        assert_eq!(store.read(&cid).unwrap(), bytes);
        // Also relocate blobs added after opening the old layout.
        let extra = b"a newly added v2 file";
        let extra_cid = compute_cid(extra);
        store.store(&extra_cid, extra).unwrap();
        let checks = AtomicUsize::new(0);
        assert!(store
            .upgrade_blob_layout_with_cancellation(|| checks.fetch_add(1, Ordering::Relaxed) == 3)
            .is_err());
        drop(store);
        let mut reopened = ContentStore::open(root.path()).unwrap();
        assert_eq!(reopened.format_version(), 3);
        assert_eq!(reopened.read(&cid).unwrap(), bytes);
        assert_eq!(reopened.read(&extra_cid).unwrap(), extra);
        assert!(reopened.upgrade_blob_layout().unwrap() > 0);
        assert_eq!(reopened.upgrade_blob_layout().unwrap(), 0);
        assert!(fs::read_dir(root.path().join("blobs")).unwrap().all(|e| e
            .unwrap()
            .path()
            .extension()
            .is_none_or(|e| e != "lz4")));
        drop(reopened);
        assert_eq!(
            ContentStore::open(root.path()).unwrap().read(&cid).unwrap(),
            bytes
        );
    }

    #[test]
    fn flat_v2_upgrade_on_configurable_filesystem() {
        let root = match std::env::var_os("DEDUP_TEST_FS_ROOT") {
            Some(path) => TempDir::new_in(path).unwrap(),
            None => TempDir::new().unwrap(),
        };
        let data = b"small flat FastCDC archive";
        let cid = make_flat_v2(root.path(), data);
        let mut store = ContentStore::open(root.path()).unwrap();
        assert_eq!(store.upgrade_blob_layout().unwrap(), 1);
        assert_eq!(store.read(&cid).unwrap(), data);
        drop(store);
        let reopened = ContentStore::open(root.path()).unwrap();
        assert_eq!(reopened.format_version(), 3);
        assert_eq!(reopened.read(&cid).unwrap(), data);
    }

    #[test]
    fn mixed_layout_reuses_flat_chunks_and_shards_new_chunks() {
        let root = TempDir::new().unwrap();
        let bytes = payload(4 * 1024 * 1024);
        let cid = make_flat_v2(root.path(), &bytes);
        let mut store = ContentStore::open(root.path()).unwrap();
        let extra = b"new flat blob after open";
        let extra_cid = compute_cid(extra);
        store.store(&extra_cid, extra).unwrap();
        store.enable_sharded_writes().unwrap();
        assert_eq!(store.read(&extra_cid).unwrap(), extra);
        let old_chunks = store.manifest(&cid).unwrap().unwrap().chunks.len();
        let mut shifted = bytes.clone();
        shifted.splice(8192..8192, b"inserted bytes".iter().copied());
        let shifted_cid = compute_cid(&shifted);
        store.store(&shifted_cid, &shifted).unwrap();
        let all = walkdir::WalkDir::new(root.path().join("blobs"))
            .into_iter()
            .map(|e| e.unwrap().into_path())
            .filter(|p| p.extension().is_some_and(|e| e == "lz4"))
            .collect::<Vec<_>>();
        assert!(
            all.len() < old_chunks + 6,
            "shared flat chunks were duplicated"
        );
        assert!(all
            .iter()
            .any(|p| p.parent().unwrap() != root.path().join("blobs")));
        assert!(all
            .iter()
            .any(|p| p.parent().unwrap() == root.path().join("blobs")));
        drop(store);
        let reopened = ContentStore::open(root.path()).unwrap();
        assert_eq!(reopened.read(&cid).unwrap(), bytes);
        assert_eq!(reopened.read(&extra_cid).unwrap(), extra);
        assert_eq!(reopened.read(&shifted_cid).unwrap(), shifted);
    }

    #[test]
    fn chunk_parallelism_preserves_sequential_fastcdc_boundaries() {
        let bytes = payload(8 * 1024 * 1024);
        let cid = compute_cid(&bytes);
        let expected = fastcdc::v2020::FastCDC::with_level(
            &bytes,
            16384,
            65536,
            262144,
            Normalization::Level1,
        )
        .map(|chunk| {
            (
                compute_cid(&bytes[chunk.offset..chunk.offset + chunk.length]).to_bytes(),
                chunk.length as u64,
            )
        })
        .collect::<Vec<_>>();
        let mut first_manifest = None;
        for threads in [1, 2, 4, 8] {
            let root = TempDir::new().unwrap();
            let mut store = ContentStore::open(root.path()).unwrap();
            store.set_chunk_parallelism(threads).unwrap();
            let stored_size = store.store(&cid, &bytes).unwrap();
            let manifest = store.manifest(&cid).unwrap().unwrap();
            assert_eq!(
                manifest
                    .chunks
                    .iter()
                    .map(|c| (c.cid.clone(), c.size))
                    .collect::<Vec<_>>(),
                expected
            );
            let encoded = bincode::serialize(&manifest).unwrap();
            if let Some(first) = &first_manifest {
                assert_eq!(first, &encoded);
            } else {
                first_manifest = Some(encoded);
            }
            assert_eq!(store.compressed_size(&cid).unwrap().unwrap(), stored_size);
            assert_eq!(store.read(&cid).unwrap(), bytes);
        }
    }

    #[test]
    fn sharded_reopen_cleans_only_owned_temporaries() {
        let root = TempDir::new().unwrap();
        let store = ContentStore::open(root.path()).unwrap();
        let bytes = b"temporary cleanup";
        let cid = compute_cid(bytes);
        store.store(&cid, bytes).unwrap();
        let directory = store.blob_path(&cid).parent().unwrap().to_path_buf();
        let temporary = directory.join(format!("{cid}.123.45.tmp"));
        let unrelated = directory.join("keep.tmp");
        fs::write(&temporary, b"unfinished").unwrap();
        fs::write(&unrelated, b"preserve").unwrap();
        drop(store);
        let store = ContentStore::open(root.path()).unwrap();
        assert!(!temporary.exists());
        assert_eq!(fs::read(unrelated).unwrap(), b"preserve");
        assert_eq!(store.read(&cid).unwrap(), bytes);
    }

    #[test]
    fn unknown_store_versions_are_rejected() {
        let tmp = TempDir::new().unwrap();
        let store = ContentStore::open(tmp.path()).unwrap();
        let txn = store.manifests.as_ref().unwrap().begin_write().unwrap();
        {
            let config = bincode::serialize(&FormatConfig {
                version: 99,
                profile: ChunkingProfile::default(),
            })
            .unwrap();
            txn.open_table(CONFIG)
                .unwrap()
                .insert("format", config.as_slice())
                .unwrap();
        }
        txn.commit().unwrap();
        drop(store);
        assert!(ContentStore::open(tmp.path()).is_err());
    }

    #[test]
    fn valid_lz4_with_wrong_content_is_rejected() {
        let tmp = TempDir::new().unwrap();
        let store = ContentStore::open(tmp.path()).unwrap();
        let data = b"original file content";
        let cid = compute_cid(data);
        store.store(&cid, data).unwrap();
        let mut encoder = FrameEncoder::new(fs::File::create(store.blob_path(&cid)).unwrap());
        encoder.write_all(b"modified file content").unwrap();
        encoder.finish().unwrap();
        assert!(store
            .read(&cid)
            .unwrap_err()
            .to_string()
            .contains("CID mismatch"));
    }

    #[test]
    fn store_and_read_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = ContentStore::open(tmp.path()).unwrap();

        let data = b"hello world, this is test content for CAS";
        let cid = compute_cid(data);

        let compressed_size = store.store(&cid, data).unwrap();
        assert!(compressed_size > 0);
        assert!(store.exists(&cid));

        let read_back = store.read(&cid).unwrap();
        assert_eq!(read_back, data);
    }

    #[test]
    fn store_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let store = ContentStore::open(tmp.path()).unwrap();

        let data = b"duplicate content";
        let cid = compute_cid(data);

        let size1 = store.store(&cid, data).unwrap();
        let size2 = store.store(&cid, data).unwrap();
        assert_eq!(size1, size2);
    }

    #[test]
    fn read_missing_blob_errors() {
        let tmp = TempDir::new().unwrap();
        let store = ContentStore::open(tmp.path()).unwrap();

        let cid = compute_cid(b"nonexistent");
        assert!(!store.exists(&cid));
        assert!(store.read(&cid).is_err());
    }

    #[test]
    fn concurrent_store_of_same_cid_is_safe() {
        use std::sync::Arc;
        use std::thread;

        let tmp = TempDir::new().unwrap();
        let store = Arc::new(ContentStore::open(tmp.path()).unwrap());
        let data = vec![7u8; 64 * 1024];
        let cid = compute_cid(&data);

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let data = data.clone();
                thread::spawn(move || {
                    store.store(&cid, &data).unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        assert!(store.exists(&cid));
        assert_eq!(store.read(&cid).unwrap(), data);

        let leftover_tmp: Vec<String> = walkdir::WalkDir::new(tmp.path().join("blobs"))
            .follow_links(false)
            .into_iter()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(
            leftover_tmp.is_empty(),
            "leftover temp files: {leftover_tmp:?}"
        );
    }

    #[test]
    fn concurrent_publication_counts_new_storage_once() {
        use std::sync::{Arc, Barrier};
        let tmp = match std::env::var_os("DEDUP_TEST_FS_ROOT") {
            Some(root) => TempDir::new_in(root).unwrap(),
            None => TempDir::new().unwrap(),
        };
        let store = Arc::new(ContentStore::open(tmp.path()).unwrap());
        let barrier = Arc::new(Barrier::new(8));
        let handles = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let data = vec![7u8; 64 * 1024];
                    barrier.wait();
                    store
                        .store_detailed(&compute_cid(&data), &data)
                        .unwrap()
                        .new_bytes
                })
            })
            .collect::<Vec<_>>();
        let new_bytes: u64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
        let data = vec![7u8; 64 * 1024];
        let cid = compute_cid(&data);
        assert_eq!(new_bytes, store.compressed_size(&cid).unwrap().unwrap());
        assert_eq!(store.read(&cid).unwrap(), data);
    }
}
