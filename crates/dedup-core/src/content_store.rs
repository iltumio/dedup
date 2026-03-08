use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lz4_flex::frame::{FrameDecoder, FrameEncoder};

use crate::cid as cid_util;
use cid::Cid;

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

        // Write to a temp file then rename for atomicity.
        let tmp_path = path.with_extension("lz4.tmp");

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
}
