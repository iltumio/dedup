//! Physical blob layout, verified deduplication and atomic publication.
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use ::cid::Cid;
use anyhow::{ensure, Context, Result};
use lz4_flex::frame::{FrameDecoder, FrameEncoder};

use crate::cid;
use crate::content_store::{lock_file, sync_dir};

static TMP_NONCE: AtomicU64 = AtomicU64::new(0);
const CACHE_ENTRIES_PER_BUCKET: usize = 64;

#[derive(Clone, PartialEq, Eq)]
struct Fingerprint {
    size: u64,
    modified: SystemTime,
    #[cfg(unix)]
    identity: (u64, i64, i64),
}
impl Fingerprint {
    fn from_metadata(meta: &fs::Metadata) -> Option<Self> {
        if !meta.is_file() {
            return None;
        }
        Some(Self {
            size: meta.len(),
            modified: meta.modified().ok()?,
            #[cfg(unix)]
            identity: {
                use std::os::unix::fs::MetadataExt;
                (meta.ino(), meta.ctime(), meta.ctime_nsec())
            },
        })
    }
}

#[derive(Default)]
struct VerifiedCache {
    entries: HashMap<Cid, Fingerprint>,
    order: VecDeque<Cid>,
}
impl VerifiedCache {
    fn insert(&mut self, cid: Cid, fingerprint: Fingerprint) {
        if let Some(entry) = self.entries.get_mut(&cid) {
            *entry = fingerprint;
            return;
        }
        if self.entries.len() == CACHE_ENTRIES_PER_BUCKET {
            self.entries.remove(&self.order.pop_front().unwrap());
        }
        self.order.push_back(cid);
        self.entries.insert(cid, fingerprint);
    }
}

#[cfg(target_os = "linux")]
struct Persistence {
    file: fs::File,
    failed: bool,
}
#[cfg(target_os = "linux")]
impl Persistence {
    fn sync_with(&mut self, sync: impl FnOnce(&fs::File) -> std::io::Result<()>) -> Result<()> {
        ensure!(
            !self.failed,
            "previous chunk persistence failure; reopen the archive before retrying"
        );
        if let Err(error) = sync(&self.file) {
            // syncfs consumes writeback errors. Serialize the barrier and poison
            // this handle so another worker cannot commit after an error was consumed.
            self.failed = true;
            return Err(error).context("failed to persist chunk batch");
        }
        Ok(())
    }
}

pub(crate) struct BlobStore {
    root: PathBuf,
    sharded: bool,
    bulk_sync: bool,
    #[cfg(target_os = "linux")]
    persistence: Option<Mutex<Persistence>>,
    // One inventory avoids a failed lookup in the huge flat directory for every
    // new chunk when recovering an interrupted layout upgrade.
    flat: Mutex<HashSet<Cid>>,
    track_flat_publications: bool,
    directories: Mutex<HashSet<PathBuf>>,
    // Publication and cache entries are coordinated per digest prefix. V2/legacy
    // OS locks still protect independent handles; v3 uses a lock per leaf shard.
    buckets: [Mutex<VerifiedCache>; 256],
    #[cfg(test)]
    size_reads: AtomicU64,
}

impl BlobStore {
    pub(crate) fn open(root: PathBuf, sharded: bool, cleanup: bool) -> Result<Self> {
        fs::create_dir_all(&root)?;
        let bulk_sync = bulk_sync_supported();
        // Open BEFORE any chunk writes: syncfs error accounting is sampled when
        // a descriptor is opened, so reopening it at commit could miss earlier failures.
        #[cfg(target_os = "linux")]
        let persistence = if bulk_sync {
            Some(Mutex::new(Persistence {
                file: fs::File::open(&root)?,
                failed: false,
            }))
        } else {
            None
        };
        let mut flat = HashSet::new();
        if cleanup {
            for entry in fs::read_dir(&root)? {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if entry.file_type()?.is_file() {
                    if let Some(stem) = name.strip_suffix(".lz4") {
                        if let Ok(cid) = cid::cid_from_string(stem) {
                            flat.insert(cid);
                        }
                    }
                    remove_temporary(&entry.path(), &name)?;
                } else if sharded && entry.file_type()?.is_dir() && is_hex(&name, 2) {
                    for leaf in fs::read_dir(entry.path())? {
                        let leaf = leaf?;
                        if leaf.file_type()?.is_dir()
                            && is_hex(&leaf.file_name().to_string_lossy(), 1)
                        {
                            for file in fs::read_dir(leaf.path())? {
                                let file = file?;
                                if file.file_type()?.is_file() {
                                    remove_temporary(
                                        &file.path(),
                                        &file.file_name().to_string_lossy(),
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(Self {
            root,
            sharded,
            bulk_sync,
            #[cfg(target_os = "linux")]
            persistence,
            flat: Mutex::new(flat),
            track_flat_publications: cleanup && !sharded,
            directories: Mutex::new(HashSet::new()),
            buckets: std::array::from_fn(|_| Mutex::new(VerifiedCache::default())),
            #[cfg(test)]
            size_reads: AtomicU64::new(0),
        })
    }

    pub(crate) fn enable_sharding(&mut self) {
        self.sharded = true;
        self.track_flat_publications = false;
    }

    fn bucket(cid: &Cid) -> usize {
        cid.hash().digest().first().copied().unwrap_or(0) as usize
    }

    fn sharded_path(&self, cid: &Cid) -> PathBuf {
        let digest = cid.hash().digest();
        self.root
            .join(format!("{:02x}", Self::bucket(cid)))
            .join(format!("{:x}", digest.get(1).copied().unwrap_or(0) >> 4))
            .join(format!("{cid}.lz4"))
    }

    pub(crate) fn path(&self, cid: &Cid) -> PathBuf {
        if self.sharded
            && !self
                .flat
                .lock()
                .expect("flat inventory poisoned")
                .contains(cid)
        {
            self.sharded_path(cid)
        } else {
            self.root.join(format!("{cid}.lz4"))
        }
    }

    pub(crate) fn reader(&self, cid: &Cid) -> Result<FrameDecoder<fs::File>> {
        Ok(FrameDecoder::new(
            fs::File::open(self.path(cid)).with_context(|| format!("blob not found: {cid}"))?,
        ))
    }

    pub(crate) fn size(&self, cid: &Cid) -> Result<u64> {
        self.metadata_size(|| fs::metadata(self.path(cid)))
    }

    /// Visit each referenced directory in its native order. In particular,
    /// exFAT can reuse its forward lookup hint instead of restarting random
    /// filename searches in a large flat directory for every manifest chunk.
    pub(crate) fn referenced_size(
        &self,
        cids: HashSet<Cid>,
        mut on_progress: impl FnMut(u64) -> Result<()>,
    ) -> Result<u64> {
        let mut directories: BTreeMap<PathBuf, HashSet<OsString>> = BTreeMap::new();
        for cid in cids {
            let path = self.path(&cid);
            directories
                .entry(path.parent().unwrap().to_path_buf())
                .or_default()
                .insert(path.file_name().unwrap().to_owned());
        }
        let mut size = 0u64;
        let mut processed = 0;
        on_progress(processed)?;
        for (directory, mut needed) in directories {
            for (visited, entry) in fs::read_dir(&directory)
                .with_context(|| format!("failed to read blob directory {}", directory.display()))?
                .enumerate()
            {
                let entry = entry?;
                if needed.remove(&entry.file_name()) {
                    size = size
                        .checked_add(self.metadata_size(|| {
                            let metadata = entry.metadata()?;
                            if metadata.file_type().is_symlink() {
                                fs::metadata(entry.path())
                            } else {
                                Ok(metadata)
                            }
                        })?)
                        .context("archive size overflow")?;
                    processed += 1;
                }
                // Also yield during directories containing mostly unreferenced
                // historical files, so cancellation does not wait for a match.
                if visited.is_multiple_of(256) || needed.is_empty() {
                    on_progress(processed)?;
                }
                if needed.is_empty() {
                    break;
                }
            }
            if let Some(missing) = needed.iter().next() {
                anyhow::bail!(
                    "referenced blob not found: {}",
                    directory.join(missing).display()
                );
            }
        }
        on_progress(processed)?;
        Ok(size)
    }

    fn metadata_size(
        &self,
        metadata: impl FnOnce() -> std::io::Result<fs::Metadata>,
    ) -> Result<u64> {
        #[cfg(test)]
        self.size_reads.fetch_add(1, Ordering::Relaxed);
        Ok(metadata()?.len())
    }

    #[cfg(test)]
    pub(crate) fn take_size_reads(&self) -> u64 {
        self.size_reads.swap(0, Ordering::Relaxed)
    }

    // Restores always verify the bytes, even when publication used the cache.
    pub(crate) fn read(&self, cid: &Cid) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        self.reader(cid)?
            .read_to_end(&mut data)
            .context("failed to decompress blob")?;
        ensure!(cid::compute_cid(&data) == *cid, "blob CID mismatch: {cid}");
        Ok(data)
    }

    fn verified_size(&self, cid: &Cid, cache: &mut VerifiedCache) -> Option<u64> {
        let metadata = fs::metadata(self.path(cid)).ok()?;
        let fingerprint = Fingerprint::from_metadata(&metadata);
        if fingerprint
            .as_ref()
            .is_some_and(|f| cache.entries.get(cid) == Some(f))
        {
            return Some(metadata.len());
        }
        // Changed, corrupt or deleted blobs never inherit a cached verification.
        if self.read(cid).is_err() {
            return None;
        }
        if let Some(fingerprint) = fingerprint {
            cache.insert(*cid, fingerprint);
        }
        Some(metadata.len())
    }

    fn ensure_directory(&self, directory: &Path) -> Result<()> {
        let known = self
            .directories
            .lock()
            .expect("blob directory cache poisoned")
            .contains(directory);
        if !known {
            fs::create_dir_all(directory)?;
            self.directories
                .lock()
                .expect("blob directory cache poisoned")
                .insert(directory.to_path_buf());
        }
        Ok(())
    }

    /// Publish precompressed, CID-verified source bytes. Chunk callers establish
    /// a data/directory durability barrier before committing their manifest.
    pub(crate) fn publish(
        &self,
        cid: &Cid,
        encoded: &[u8],
        sync_publication: bool,
    ) -> Result<(u64, bool)> {
        self.publish_with(cid, sync_publication, |file| {
            file.write_all(encoded)?;
            Ok(encoded.len() as u64)
        })
    }

    // Legacy blobs can be many gigabytes: compress straight into the temporary
    // file instead of retaining a second whole-file compressed buffer in RAM.
    pub(crate) fn store(&self, cid: &Cid, data: &[u8]) -> Result<(u64, bool)> {
        self.publish_with(cid, true, |file| {
            let mut encoder = FrameEncoder::new(file);
            encoder.write_all(data)?;
            Ok(encoder.finish()?.metadata()?.len())
        })
    }

    fn publish_with(
        &self,
        cid: &Cid,
        sync_publication: bool,
        write: impl FnOnce(&mut fs::File) -> Result<u64>,
    ) -> Result<(u64, bool)> {
        let mut cache = self.buckets[Self::bucket(cid)]
            .lock()
            .expect("blob cache poisoned");
        if let Some(size) = self.verified_size(cid, &mut cache) {
            return Ok((size, false));
        }
        let path = self.path(cid);
        let directory = path.parent().unwrap();
        self.ensure_directory(directory)?;
        let nonce = TMP_NONCE.fetch_add(1, Ordering::Relaxed);
        let tmp = directory.join(format!("{cid}.{}.{nonce}.tmp", std::process::id()));
        let mut renamed = false;
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            let size = write(&mut file)?;
            // On Linux the manifest barrier synchronizes data and metadata for
            // the entire backing filesystem. Elsewhere retain per-blob fsync.
            // Legacy callers still need durability before returning this blob.
            if sync_publication || !self.bulk_sync {
                file.sync_all()?;
            }
            let _publication = lock_file(&directory.join(".publish.lock"))?;
            if let Some(size) = self.verified_size(cid, &mut cache) {
                return Ok((size, false));
            }
            let created = !path.is_file();
            fs::rename(&tmp, &path).with_context(|| format!("failed to publish blob {cid}"))?;
            renamed = true;
            if self.track_flat_publications {
                self.flat
                    .lock()
                    .expect("flat inventory poisoned")
                    .insert(*cid);
            }
            if sync_publication {
                sync_dir(directory)?;
            }
            if let Some(fingerprint) = Fingerprint::from_metadata(&fs::metadata(&path)?) {
                cache.insert(*cid, fingerprint);
            }
            Ok((size, created))
        })();
        // A successful rename already removed the temporary name. Avoid an
        // expensive unsuccessful exFAT directory search after every publication.
        if !renamed {
            let _ = fs::remove_file(&tmp);
        }
        result
    }

    pub(crate) fn sync_directories(&self, directories: HashSet<PathBuf>) -> Result<()> {
        #[cfg(target_os = "linux")]
        if self.bulk_sync {
            use std::os::fd::AsRawFd;
            let mut persistence = self
                .persistence
                .as_ref()
                .unwrap()
                .lock()
                .expect("persistence barrier poisoned");
            if directories.is_empty() {
                return persistence.sync_with(|_| Ok(()));
            }
            // Linux syncfs waits for data AND filesystem metadata, including
            // new shard ancestors. Commit the manifest only after success.
            // https://man7.org/linux/man-pages/man2/sync.2.html
            return persistence.sync_with(|file| {
                // SAFETY: `file` owns a valid descriptor throughout this call.
                if unsafe { libc::syncfs(file.as_raw_fd()) } != 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        {
            let mut all = directories.clone();
            for directory in directories {
                let mut parent = directory.parent();
                while let Some(path) = parent {
                    if !path.starts_with(&self.root) {
                        break;
                    }
                    all.insert(path.to_path_buf());
                    parent = path.parent();
                }
            }
            let mut all = all.into_iter().collect::<Vec<_>>();
            all.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
            for directory in all {
                sync_dir(&directory)?;
            }
            Ok(())
        }
    }

    /// The caller commits format v3 BEFORE moving any names. A partial upgrade
    /// remains readable through the flat inventory and can be resumed on reopen.
    pub(crate) fn upgrade(&mut self, should_cancel: impl Fn() -> bool) -> Result<u64> {
        self.enable_sharding();
        let mut moved = 0;
        let flat = self
            .flat
            .lock()
            .expect("flat inventory poisoned")
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for cid in flat {
            ensure!(
                !should_cancel(),
                "layout upgrade cancelled; run again to resume"
            );
            let from = self.root.join(format!("{cid}.lz4"));
            let to = self.sharded_path(&cid);
            let directory = to.parent().unwrap();
            self.ensure_directory(directory)?;
            // exFAT has no journal. Keep the old name and data until the copy
            // and all destination directory entries are durable.
            fs::copy(&from, &to).with_context(|| format!("failed to relocate blob {cid}"))?;
            fs::OpenOptions::new().write(true).open(&to)?.sync_all()?;
            // Persist the destination before forgetting the old name. No file
            // contents, CIDs or manifests change during this operation.
            self.sync_directories(HashSet::from([directory.to_path_buf(), self.root.clone()]))?;
            fs::remove_file(&from)?;
            self.flat
                .lock()
                .expect("flat inventory poisoned")
                .remove(&cid);
            sync_dir(&self.root)?;
            moved += 1;
        }
        Ok(moved)
    }
}

fn is_hex(name: &str, length: usize) -> bool {
    name.len() == length && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn remove_temporary(path: &Path, name: &str) -> Result<()> {
    let parts = name.split('.').collect::<Vec<_>>();
    if parts.len() == 4
        && parts[3] == "tmp"
        && cid::cid_from_string(parts[0]).is_ok()
        && parts[1].parse::<u32>().is_ok()
        && parts[2].parse::<u64>().is_ok()
    {
        fs::remove_file(path)?;
    }
    Ok(())
}

// Older kernels do not report filesystem writeback errors through syncfs.
// Preserve per-file fsync when support cannot be established.
#[cfg(target_os = "linux")]
fn bulk_sync_supported() -> bool {
    let Ok(release) = fs::read_to_string("/proc/sys/kernel/osrelease") else {
        return false;
    };
    let mut parts = release.trim().split('.');
    match (
        parts.next().and_then(|p| p.parse::<u32>().ok()),
        parts.next().and_then(|p| p.parse::<u32>().ok()),
    ) {
        (Some(major), Some(minor)) => major > 5 || (major == 5 && minor >= 8),
        _ => false,
    }
}
#[cfg(not(target_os = "linux"))]
fn bulk_sync_supported() -> bool {
    false
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn failed_filesystem_barrier_blocks_later_commits() {
        let root = tempfile::TempDir::new().unwrap();
        let mut persistence = Persistence {
            file: fs::File::open(root.path()).unwrap(),
            failed: false,
        };
        assert!(persistence
            .sync_with(|_| Err(std::io::Error::from_raw_os_error(libc::EIO)))
            .is_err());
        assert!(persistence
            .sync_with(|_| panic!("must not consume another error after failed persistence"))
            .is_err());
    }
}
