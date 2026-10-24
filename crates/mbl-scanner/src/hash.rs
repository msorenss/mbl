//! Quick content hash for detecting moved/renamed files without reading them fully.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use xxhash_rust::xxh3::Xxh3;

const CHUNK: u64 = 64 * 1024;

/// xxh3 over the first, middle and last 64 KiB plus the file size (8 bytes, big-endian).
pub fn quick_hash(path: &str) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let mut hasher = Xxh3::new();
    hasher.update(&size.to_le_bytes());
    let mut buf = vec![0u8; CHUNK as usize];
    let offsets =
        if size <= 3 * CHUNK { vec![0] } else { vec![0, size / 2 - CHUNK / 2, size - CHUNK] };
    for offset in offsets {
        file.seek(SeekFrom::Start(offset))?;
        let limit = if size <= 3 * CHUNK { size } else { CHUNK };
        let mut taken = (&mut file).take(limit);
        loop {
            let n = taken.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    }
    Ok(hasher.digest().to_be_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_content_same_hash() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        let data: Vec<u8> = (0..500_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&a, &data).unwrap();
        std::fs::write(&b, &data).unwrap();
        let ha = quick_hash(a.to_str().unwrap()).unwrap();
        assert_eq!(ha.len(), 8);
        assert_eq!(ha, quick_hash(b.to_str().unwrap()).unwrap());

        let mut changed = data.clone();
        changed[0] ^= 1;
        std::fs::write(&b, &changed).unwrap();
        assert_ne!(ha, quick_hash(b.to_str().unwrap()).unwrap());

        std::fs::write(&b, b"tiny").unwrap();
        assert_eq!(quick_hash(b.to_str().unwrap()).unwrap().len(), 8);
    }
}
