use core::fmt;
use std::{path::PathBuf, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    format::{DataFormat, DataFormatReader, DataReadError},
    state::InternedId,
};

/// A BinaryEntry is the location to a binary with an optional shasum
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinaryEntry {
    pub source: BinarySource,
    pub sha256: Option<String>,
    pub size: Option<u64>,
}

impl BinaryEntry {
    pub fn with_api_path(&self, storage_id: InternedId, binary_id: InternedId) -> BinaryEntry {
        match &self.source {
            BinarySource::Url(_) => self.clone(),
            BinarySource::Path(_) => BinaryEntry {
                source: BinarySource::Path(PathBuf::from(format!(
                    "/content/storage/{storage_id}/binaries/{binary_id}"
                ))),
                sha256: None,
                size: None,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum BinarySource {
    Url(url::Url),
    Path(PathBuf),
}

impl fmt::Display for BinarySource {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
            BinarySource::Url(url) => url.serialize(serializer),
            BinarySource::Path(path) => path.to_string_lossy().serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for BinarySource {
    fn deserialize<D>(deserializer: D) -> Result<BinarySource, D::Error>
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
