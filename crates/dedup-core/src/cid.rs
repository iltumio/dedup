use anyhow::Result;
use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};

/// Multicodec code for "raw" binary content.
const RAW_CODEC: u64 = 0x55;

/// Compute a CIDv1 from file content bytes using BLAKE3.
///
/// The resulting CID uses:
/// - CIDv1 format
/// - Raw binary codec (0x55)
/// - BLAKE3-256 multihash
pub fn compute_cid(data: &[u8]) -> Cid {
    let hash = Code::Blake3_256.digest(data);
    Cid::new_v1(RAW_CODEC, hash)
}

/// Encode a CID to a base32-lower string (suitable for filenames).
pub fn cid_to_string(cid: &Cid) -> String {
    // Default Display for CID uses base32lower for v1
    cid.to_string()
}

/// Parse a CID from its string representation.
pub fn cid_from_string(s: &str) -> Result<Cid> {
    let cid = Cid::try_from(s)?;
    Ok(cid)
}

/// Convert a CID to its raw byte representation.
pub fn cid_to_bytes(cid: &Cid) -> Vec<u8> {
    cid.to_bytes()
}

/// Parse a CID from raw bytes.
pub fn cid_from_bytes(bytes: &[u8]) -> Result<Cid> {
    let cid = Cid::try_from(bytes)?;
    Ok(cid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_cid() {
        let data = b"hello world";
        let cid1 = compute_cid(data);
        let cid2 = compute_cid(data);
        assert_eq!(cid1, cid2);
    }

    #[test]
    fn different_content_different_cid() {
        let cid1 = compute_cid(b"hello");
        let cid2 = compute_cid(b"world");
        assert_ne!(cid1, cid2);
    }

    #[test]
    fn roundtrip_string() {
        let cid = compute_cid(b"test data");
        let s = cid_to_string(&cid);
        let parsed = cid_from_string(&s).unwrap();
        assert_eq!(cid, parsed);
    }

    #[test]
    fn roundtrip_bytes() {
        let cid = compute_cid(b"test data");
        let bytes = cid_to_bytes(&cid);
        let parsed = cid_from_bytes(&bytes).unwrap();
        assert_eq!(cid, parsed);
    }
}
