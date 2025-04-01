use std::{
    fmt::Display,
    io,
    path::{Path, PathBuf},
    str::FromStr,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    format::{DataFormat, DataFormatReader, DataReadError},
    state::{InternedId, NetworkId},
    util::sha256_file,
};

/// A BinaryEntry is the location to a binary with an optional shasum
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct BinaryEntry {
    pub source: BinarySource,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

impl BinaryEntry {
    pub fn with_api_path(
        &self,
        network: NetworkId,
        storage_id: InternedId,
        binary_id: InternedId,
    ) -> BinaryEntry {
        match &self.source {
            BinarySource::Url(_) => self.clone(),
            BinarySource::Path(_) => BinaryEntry {
                source: BinarySource::Path(PathBuf::from(format!(
                    "/content/storage/{network}/{storage_id}/binaries/{binary_id}"
                ))),
                sha256: self.sha256.clone(),
                size: self.size,
            },
        }
    }

    /// Determines if the file is fetched from the control plane
    pub fn is_api_file(&self) -> bool {
        matches!(self.source, BinarySource::Path(_))
    }

    /// Check if the sha256 is a valid sha256 hash
    pub fn check_sha256(&self) -> bool {
        self.sha256
            .as_ref()
            .map(|s| s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit()))
            .unwrap_or(false)
    }

    /// Check if the given file has the same size as the size in the
    /// BinaryEntry, return the file's size if it does not match
    pub fn check_file_size(&self, path: &Path) -> Result<Option<u64>, io::Error> {
        let Some(size) = self.size else {
            return Ok(None);
        };
        Ok((path.metadata()?.len() != size).then_some(size))
    }

    /// Check if the given file has the same sha256 as the sha256 in the
    /// BinaryEntry, return the file's sha256 if it does not match
    pub fn check_file_sha256(&self, path: &PathBuf) -> Result<Option<String>, io::Error> {
        let Some(sha256) = &self.sha256 else {
            return Ok(None);
        };
        let file_hash = sha256_file(path)?;
        Ok((&file_hash != sha256).then_some(file_hash))
    }
}

impl Display for BinaryEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "source: {}", self.source)?;
        writeln!(f, "sha256: {}", self.sha256.as_deref().unwrap_or("not set"))?;
        writeln!(
            f,
            "size: {}",
            self.size
                .map(|s| format!("{s} bytes"))
                .as_deref()
                .unwrap_or("not set")
        )?;
        if let BinarySource::Path(path) = &self.source {
            if let Ok(time) = path.metadata().and_then(|m| m.modified()) {
                writeln!(
                    f,
                    "last modified: {}",
                    DateTime::<Utc>::from(time).naive_local()
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BinarySource {
    Url(url::Url),
    Path(PathBuf),
}

impl Display for BinarySource {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            BinarySource::Url(url) => write!(f, "{}", url),
            BinarySource::Path(path) => write!(f, "{}", path.display()),
        }
    }
}

impl FromStr for BinarySource {
    type Err = url::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("http://") || s.starts_with("https://") {
            Ok(BinarySource::Url(url::Url::parse(s)?))
        } else {
            Ok(BinarySource::Path(PathBuf::from(s)))
        }
    }
}

impl Serialize for BinarySource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self {
            BinarySource::Url(url) => url.to_string().serialize(serializer),
            BinarySource::Path(path) => path.to_string_lossy().serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for BinarySource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl DataFormat for BinaryEntry {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        Ok(self.source.to_string().write_data(writer)?
            + self.sha256.write_data(writer)?
            + self.size.write_data(writer)?)
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "BinaryEntry",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        Ok(BinaryEntry {
            source: String::read_data(reader, &())?
                .parse::<BinarySource>()
                .map_err(|e| DataReadError::Custom(e.to_string()))?,
            sha256: reader.read_data(&())?,
            size: reader.read_data(&())?,
        })
    }
}
