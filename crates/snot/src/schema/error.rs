use std::path::PathBuf;

use axum::http::StatusCode;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use snot_common::rpc::error::PrettyError;
use strum_macros::AsRefStr;
use thiserror::Error;
use url::Url;

use crate::error::CommandError;

#[derive(Debug, Error, AsRefStr)]
pub enum StorageError {
    #[error("command error id: `{0}`: {1}")]
    Command(CommandError, String),
    #[error("error mkdirs for storage generation id: `{0}`: {1}")]
    GenerateStorage(String, #[source] std::io::Error),
    #[error("error generating genesis id: `{0}`: {1}")]
    FailedToGenGenesis(String, #[source] std::io::Error),
    #[error("error fetching genesis block id: `{0}` url: `{1}`: {2}")]
    FailedToFetchGenesis(String, Url, #[source] reqwest::Error),
    #[error("error writing genesis block id: `{0}`: {1}")]
    FailedToWriteGenesis(String, #[source] std::io::Error),
    #[error("error taring ledger id: `{0}`: {1}")]
    FailedToTarLedger(String, #[source] std::io::Error),
    #[error("the specified storage ID {0} doesn't exist, and no generation params were specified")]
    NoGenerationParams(String),
    #[error("error reading balances {0:#?}: {1}")]
    ReadBalances(PathBuf, #[source] std::io::Error),
    #[error("error parsing balances {0:#?}: {1}")]
    ParseBalances(PathBuf, #[source] serde_json::Error),
}

impl StorageError {
    fn status_code(&self) -> StatusCode {
        match self {
            StorageError::Command(e, _) => e.status_code(),
            StorageError::FailedToFetchGenesis(_, _, _) => StatusCode::MISDIRECTED_REQUEST,
            StorageError::NoGenerationParams(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl Serialize for StorageError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PrettyError::from(self).serialize(serializer)
    }
}

#[derive(Debug, Error)]
#[error("invalid node target string")]
pub struct NodeTargetError;

impl NodeTargetError {
    fn status_code(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum KeySourceError {
    #[error("invalid key source string")]
    InvalidKeySource,
    #[error("invalid committee index: {0}")]
    InvalidCommitteeIndex(#[source] std::num::ParseIntError),
}

impl KeySourceError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidKeySource => StatusCode::BAD_REQUEST,
            Self::InvalidCommitteeIndex(_) => StatusCode::BAD_REQUEST,
        }
    }
}

impl Serialize for KeySourceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PrettyError::from(self).serialize(serializer)
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum SchemaError {
    #[error("key source error: {0}")]
    KeySource(#[from] KeySourceError),
    #[error("node target error: {0}")]
    NodeTarget(#[from] NodeTargetError),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
}

impl SchemaError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::KeySource(e) => e.status_code(),
            Self::NodeTarget(e) => e.status_code(),
            Self::Storage(e) => e.status_code(),
        }
    }
}

impl Serialize for SchemaError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", self.as_ref())?;

        match self {
            Self::KeySource(e) => state.serialize_field("error", e),
            Self::NodeTarget(e) => state.serialize_field("error", &e.to_string()),
            Self::Storage(e) => state.serialize_field("error", e),
        }?;

        state.end()
    }
}
