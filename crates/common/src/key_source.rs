use core::fmt;
use std::str::FromStr;

use http::StatusCode;
use lazy_static::lazy_static;
use serde::{de::Visitor, Deserialize, Deserializer, Serialize};
use strum_macros::AsRefStr;
use thiserror::Error;

use crate::{format::*, impl_into_status_code, state::InternedId};

#[derive(Debug, Error, AsRefStr)]
pub enum KeySourceError {
    #[error("invalid key source string")]
    InvalidKeySource,
    #[error("invalid committee index: {0}")]
    InvalidCommitteeIndex(#[source] std::num::ParseIntError),
}

impl_into_status_code!(KeySourceError, |value| match value {
    InvalidKeySource => StatusCode::BAD_REQUEST,
    InvalidCommitteeIndex(_) => StatusCode::BAD_REQUEST,
});

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum KeySource {
    /// Private key owned by the agent
    Local,
    /// APrivateKey1zkp...
    PrivateKeyLiteral(String),
    /// aleo1...
    PublicKeyLiteral(String),
    /// program_name1.aleo
    ProgramLiteral(String),
    /// committee.0 or committee.$ (for replicas)
    Committee(Option<usize>),
    /// accounts.0 or accounts.$ (for replicas)
    Named(InternedId, Option<usize>),
}

impl<'de> Deserialize<'de> for KeySource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct KeySourceVisitor;

        impl<'de> Visitor<'de> for KeySourceVisitor {
            type Value = KeySource;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "a string that represents an aleo private/public key, or a file from storage",
                )
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                KeySource::from_str(v).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(KeySourceVisitor)
    }
}

impl Serialize for KeySource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

lazy_static! {
    pub static ref ACCOUNTS_KEY_ID: InternedId = InternedId::from_str("accounts").unwrap();
}

impl FromStr for KeySource {
    type Err = KeySourceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // use KeySource::Literal(String) when the string is 59 characters long and
        // starts with "APrivateKey1zkp" use KeySource::Commitee(Option<usize>)
        // when the string is "committee.0" or "committee.$"
        // use KeySource::Named(String, Option<usize>) when the string is "\w+.0" or
        // "\w+.$"

        if s == "local" {
            return Ok(KeySource::Local);
        // aleo private key
        } else if s.len() == 59 && s.starts_with("APrivateKey1") {
            return Ok(KeySource::PrivateKeyLiteral(s.to_string()));
        // aleo public key
        } else if s.len() == 63 && s.starts_with("aleo1") {
            return Ok(KeySource::PublicKeyLiteral(s.to_string()));

        // committee key
        } else if let Some(index) = s.strip_prefix("committee.") {
            if index == "$" {
                return Ok(KeySource::Committee(None));
            }
            let replica = index
                .parse()
                .map_err(KeySourceError::InvalidCommitteeIndex)?;
            return Ok(KeySource::Committee(Some(replica)));
        }

        // named key (using regex with capture groups)
        lazy_static! {
            static ref NAMED_KEYSOURCE_REGEX: regex::Regex =
                regex::Regex::new(r"^(?P<name>[A-Za-z0-9][A-Za-z0-9\-_.]{0,63})\.(?P<idx>\d+|\$)$")
                    .unwrap();
            static ref NAMED_PROGRAM_REGEX: regex::Regex =
                regex::Regex::new(r"^[A-Za-z0-9_]{1,256}\.aleo$").unwrap();
        }

        if NAMED_PROGRAM_REGEX.is_match(s) {
            return Ok(KeySource::ProgramLiteral(s.to_string()));
        }

        let groups = NAMED_KEYSOURCE_REGEX
            .captures(s)
            .ok_or(KeySourceError::InvalidKeySource)?;
        let name = InternedId::from_str(groups.name("name").unwrap().as_str())
            .map_err(|_| KeySourceError::InvalidKeySource)?;
        let idx = match groups.name("idx").unwrap().as_str() {
            "$" => None,
            idx => Some(idx.parse().map_err(KeySourceError::InvalidCommitteeIndex)?),
        };
        Ok(KeySource::Named(name, idx))
    }
}

impl fmt::Display for KeySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                KeySource::Local => "local".to_owned(),
                KeySource::PrivateKeyLiteral(key) => key.to_owned(),
                KeySource::ProgramLiteral(key) => key.to_owned(),
                KeySource::PublicKeyLiteral(key) => key.to_owned(),
                KeySource::Committee(None) => "committee.$".to_owned(),
                KeySource::Committee(Some(idx)) => {
                    format!("committee.{}", idx)
                }
                KeySource::Named(name, None) => format!("{}.{}", name, "$"),
                KeySource::Named(name, Some(idx)) => {
                    format!("{}.{}", name, idx)
                }
            }
        )
    }
}

impl DataFormat for KeySource {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1u8;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, DataWriteError> {
        Ok(match self {
            KeySource::Local => writer.write_data(&0u8)?,
            KeySource::PrivateKeyLiteral(key) => {
                writer.write_data(&1u8)? + writer.write_data(key)?
            }
            KeySource::Committee(None) => writer.write_data(&2u8)?,
            KeySource::Committee(Some(idx)) => {
                // save a byte by making this a separate case
                writer.write_data(&3u8)? + writer.write_data(idx)?
            }
            KeySource::Named(name, None) => writer.write_data(&4u8)? + writer.write_data(name)?,
            KeySource::Named(name, Some(idx)) => {
                // save a byte by making this a separate case
                writer.write_data(&5u8)? + writer.write_data(name)? + writer.write_data(idx)?
            }
            KeySource::PublicKeyLiteral(key) => {
                writer.write_data(&6u8)? + writer.write_data(key)?
            }
            KeySource::ProgramLiteral(key) => writer.write_data(&7u8)? + writer.write_data(key)?,
        })
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "KeySource",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        match reader.read_data(&())? {
            0u8 => Ok(KeySource::Local),
            1u8 => Ok(KeySource::PrivateKeyLiteral(reader.read_data(&())?)),
            2u8 => Ok(KeySource::Committee(None)),
            3u8 => Ok(KeySource::Committee(Some(reader.read_data(&())?))),
            4u8 => Ok(KeySource::Named(reader.read_data(&())?, None)),
            5u8 => Ok(KeySource::Named(
                reader.read_data(&())?,
                Some(reader.read_data(&())?),
            )),
            6u8 => Ok(KeySource::PublicKeyLiteral(reader.read_data(&())?)),
            7u8 => Ok(KeySource::ProgramLiteral(reader.read_data(&())?)),
            n => Err(DataReadError::Custom(format!("invalid KeySource tag {n}"))),
        }
    }
}

impl KeySource {
    /// Add an index to a key source only if it did not have an index before
    pub fn with_index(&self, idx: usize) -> Self {
        match self {
            KeySource::Committee(None) => KeySource::Committee(Some(idx)),
            KeySource::Named(name, None) => KeySource::Named(*name, Some(idx)),
            _ => self.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::key_source::KeySource;
    use crate::key_source::ACCOUNTS_KEY_ID;

    #[test]
    fn test_key_source_deserialization() {
        assert_eq!(
            serde_yaml::from_str::<KeySource>("committee.0").unwrap(),
            KeySource::Committee(Some(0))
        );
        assert_eq!(
            serde_yaml::from_str::<KeySource>("committee.100").unwrap(),
            KeySource::Committee(Some(100))
        );
        assert_eq!(
            serde_yaml::from_str::<KeySource>("committee.$").unwrap(),
            KeySource::Committee(None)
        );

        assert_eq!(
            serde_yaml::from_str::<KeySource>("accounts.0").unwrap(),
            KeySource::Named(*ACCOUNTS_KEY_ID, Some(0))
        );
        assert_eq!(
            serde_yaml::from_str::<KeySource>("accounts.$").unwrap(),
            KeySource::Named(*ACCOUNTS_KEY_ID, None)
        );

        assert_eq!(
            serde_yaml::from_str::<KeySource>(
                "APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH"
            )
            .unwrap(),
            KeySource::PrivateKeyLiteral(
                "APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH".to_string()
            )
        );

        assert_eq!(
            serde_yaml::from_str::<KeySource>(
                "aleo1ekc03f2vwemtpksckhrcl7mv4t7sm6ykldwldvvlysqt2my9zygqfhndya"
            )
            .unwrap(),
            KeySource::PublicKeyLiteral(
                "aleo1ekc03f2vwemtpksckhrcl7mv4t7sm6ykldwldvvlysqt2my9zygqfhndya".to_string()
            )
        );

        assert!(serde_yaml::from_str::<KeySource>("committee.-100").is_err(),);
        assert!(serde_yaml::from_str::<KeySource>("accounts.-100").is_err(),);
        assert!(serde_yaml::from_str::<KeySource>("accounts._").is_err(),);
        assert!(serde_yaml::from_str::<KeySource>("accounts.*").is_err(),);
    }
}
