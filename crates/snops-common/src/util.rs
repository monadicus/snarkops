use std::{
    ffi::OsStr,
    fmt::Debug,
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

/// A wrapper struct that has an "opaque" `Debug` implementation for types
/// that do not implement `Debug`.
pub struct OpaqueDebug<T>(pub T);

impl<T> Debug for OpaqueDebug<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("(...)")
    }
}

impl<T> std::ops::Deref for OpaqueDebug<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for OpaqueDebug<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Calculate the SHA-256 hash of a file.
pub fn sha256_file(path: &PathBuf) -> Result<String, std::io::Error> {
    let mut digest = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    let mut buffer = [0; 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }

    Ok(format!("{:x}", digest.finalize()))
}

pub fn parse_file_from_extension<T: DeserializeOwned>(
    path: &Path,
    file: File,
) -> Result<T, Box<dyn core::error::Error>> {
    // TODO: toml
    let reader = BufReader::new(file);
    let ext = path.extension().and_then(OsStr::to_str).unwrap_or_else(|| {
        tracing::warn!("invalid parse extension; falling back to yaml");
        "yaml"
    });

    Ok(match ext {
        "yaml" | "yml" => serde_yaml::from_reader(reader)?,
        "json" => serde_yaml::from_reader(reader)?,
        _ => unimplemented!("unknown parse extension {ext}"),
    })
}
