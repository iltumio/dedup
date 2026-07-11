use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use lz4_flex::frame::{FrameDecoder, FrameEncoder};

use crate::cid as cid_util;
use cid::Cid;

/// Process-wide counter making temp blob names unique per concurrent writer.
static TMP_NONCE: AtomicU64 = AtomicU64::new(0);

/// Flat blob store that keeps LZ4-compressed files keyed by CID.
///
/// Layout:
/// ```text
/// <root>/
///   blobs/
///     <cid_string>.lz4
/// ```
pub struct ContentStore {
    blobs_dir: PathBuf,
}

impl ContentStore {
    /// Open or create a content store at the given root directory.
    pub fn open(root: &Path) -> Result<Self> {
        let blobs_dir = root.join("blobs");
        fs::create_dir_all(&blobs_dir)
            .with_context(|| format!("failed to create blobs dir: {}", blobs_dir.display()))?;
        Ok(Self { blobs_dir })
    }

    /// Return the on-disk path for a given CID.
    fn blob_path(&self, cid: &Cid) -> PathBuf {
        let name = cid_util::cid_to_string(cid);
        self.blobs_dir.join(format!("{name}.lz4"))
    }

    /// Check whether a blob for this CID already exists on disk.
    pub fn exists(&self, cid: &Cid) -> bool {
        self.blob_path(cid).exists()
    }

    /// Store data as an LZ4-compressed blob. Returns compressed size in bytes.
    ///
    /// If a blob with this CID already exists, this is a no-op and returns
    /// the existing file's size.
    pub fn store(&self, cid: &Cid, data: &[u8]) -> Result<u64> {
        let path = self.blob_path(cid);

        if path.exists() {
            let meta = fs::metadata(&path)?;
            return Ok(meta.len());
        }

        // Unique temp name per writer, then atomic rename: two threads storing
        // the same CID write distinct temp files and both rename to identical
        // content, so last-writer-wins is safe.
        let nonce = TMP_NONCE.fetch_add(1, Ordering::Relaxed);
        let tmp_path = self.blobs_dir.join(format!(
            "{}.{}.{}.tmp",
            cid_util::cid_to_string(cid),
            std::process::id(),
            nonce
        ));

        let file = fs::File::create(&tmp_path)
            .with_context(|| format!("failed to create temp blob: {}", tmp_path.display()))?;

        let mut encoder = FrameEncoder::new(file);
        encoder
            .write_all(data)
            .context("failed to write compressed data")?;
        encoder.finish().context("failed to finalize LZ4 frame")?;

        fs::rename(&tmp_path, &path).with_context(|| {
            format!(
                "failed to rename {} -> {}",
                tmp_path.display(),
                path.display()
            )
        })?;

        let meta = fs::metadata(&path)?;
        Ok(meta.len())
    }

    /// Read and decompress a blob by CID. Returns the original file bytes.
    pub fn read(&self, cid: &Cid) -> Result<Vec<u8>> {
        let path = self.blob_path(cid);
        let file =
            fs::File::open(&path).with_context(|| format!("blob not found: {}", path.display()))?;

        let mut decoder = FrameDecoder::new(file);
        let mut data = Vec::new();
        decoder
            .read_to_end(&mut data)
            .context("failed to decompress blob")?;
        Ok(data)
    }

    /// Return the compressed size on disk for a blob, or None if it doesn't exist.
    pub fn compressed_size(&self, cid: &Cid) -> Result<Option<u64>> {
        let path = self.blob_path(cid);
        if !path.exists() {
            return Ok(None);
        }
        let meta = fs::metadata(&path)?;
        Ok(Some(meta.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cid::compute_cid;
    use tempfile::TempDir;

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

        let leftover_tmp: Vec<String> = fs::read_dir(tmp.path().join("blobs"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(
            leftover_tmp.is_empty(),
            "leftover temp files: {leftover_tmp:?}"
        );
    }
}
