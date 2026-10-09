//! Run with `cargo run -p dedup-core --release --example fastcdc_benchmark -- [directory]`.
use std::collections::BTreeSet;
use std::fs;
use std::time::Instant;

use anyhow::Result;
use dedup_core::{cid::compute_cid, ChunkingProfile, ContentStore};
use tempfile::TempDir;

fn main() -> Result<()> {
    let data = match std::env::args_os().nth(1) {
        Some(path) => walkdir::WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .map(|entry| {
                let entry = entry?;
                if entry.file_type().is_file() {
                    Ok(Some(fs::read(entry.path())?))
                } else {
                    Ok(None)
                }
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
        None => synthetic_data(),
    };
    let total = data.iter().map(|d| d.len() as u64).sum::<u64>();
    println!(
        "Input: {} files, {:.2} MiB",
        data.len(),
        total as f64 / 1_048_576.0
    );
    let threads = std::env::var("DEDUP_BENCH_THREADS")
        .unwrap_or_else(|_| "4".into())
        .parse::<usize>()?;
    let filesystem = std::env::var_os("DEDUP_BENCH_FS_ROOT");
    let profile_filter = std::env::var("DEDUP_BENCH_PROFILE").ok();
    println!("Chunk CPU threads: {threads}");
    println!("profile       write MiB/s  read MiB/s  payload MiB  index MiB  blobs");
    for (label, profile) in [
        ("whole-file", None),
        ("FastCDC 64K", Some(ChunkingProfile::default())),
        (
            "FastCDC 256K",
            Some(ChunkingProfile {
                version: 1,
                min_size: 64 * 1024,
                avg_size: 256 * 1024,
                max_size: 1024 * 1024,
            }),
        ),
    ] {
        if profile_filter
            .as_ref()
            .is_some_and(|filter| !label.contains(filter))
        {
            continue;
        }
        let root = match &filesystem {
            Some(path) => TempDir::new_in(path)?,
            None => TempDir::new()?,
        };
        let mut store = if let Some(profile) = profile {
            ContentStore::open_with_profile(root.path(), profile)?
        } else {
            fs::create_dir(root.path().join("blobs"))?;
            ContentStore::open(root.path())?
        };
        if profile.is_some() {
            store.set_chunk_parallelism(threads)?;
        }
        let start = Instant::now();
        let mut cids = BTreeSet::new();
        for bytes in &data {
            let cid = compute_cid(bytes);
            store.store(&cid, bytes)?;
            cids.insert(cid);
        }
        let write_seconds = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let mut restored = 0;
        for cid in cids {
            restored += store.copy_to(&cid, std::io::sink())?;
        }
        let read_seconds = start.elapsed().as_secs_f64();
        let mut payload = 0;
        let mut count = 0;
        for entry in walkdir::WalkDir::new(root.path().join("blobs")).follow_links(false) {
            let entry = entry?;
            if entry.path().extension().is_some_and(|ext| ext == "lz4") {
                payload += entry.metadata()?.len();
                count += 1;
            }
        }
        let index = fs::metadata(root.path().join("content.redb"))
            .map(|m| m.len())
            .unwrap_or(0);
        println!(
            "{label:13} {:11.2} {:11.2} {:12.2} {:10.2} {count:6}",
            total as f64 / 1_048_576.0 / write_seconds,
            restored as f64 / 1_048_576.0 / read_seconds,
            payload as f64 / 1_048_576.0,
            index as f64 / 1_048_576.0
        );
    }
    Ok(())
}

fn synthetic_data() -> Vec<Vec<u8>> {
    let mut seed = 0x1a2b3c4d5e6f7788u64;
    let base = (0..8 * 1024 * 1024)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as u8
        })
        .collect::<Vec<_>>();
    let mut result = vec![base.clone(), base.clone()];
    for n in 1..=3 {
        let mut version = base.clone();
        version.splice(n * 1024 * 1024..n * 1024 * 1024, vec![n as u8; 333]);
        result.push(version);
    }
    for n in 0..128 {
        result.push(format!("small file {n}\n").into_bytes());
    }
    result
}
