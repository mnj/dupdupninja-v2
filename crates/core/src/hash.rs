use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::error::Result;

pub fn blake3_file(path: &Path) -> Result<[u8; 32]> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();

    let mut buf = [0u8; 1024 * 128];
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }

    Ok(*hasher.finalize().as_bytes())
}

