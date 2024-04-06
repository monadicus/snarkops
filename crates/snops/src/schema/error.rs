use std::path::PathBuf;

use axum::http::StatusCode;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use snops_common::{impl_into_status_code, impl_serialize_pretty_error, rpc::error::PrettyError};
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
    #[error("error loading checkpoints: {0}")]
    CheckpointManager(#[from] checkpoint::errors::ManagerLoadError),
}

impl_into_status_code!(StorageError, |value| match value {
    Command(e, _) => e.into(),
    FailedToFetchGenesis(_, _, _) => StatusCode::MISDIRECTED_REQUEST,
    NoGenerationParams(_) => StatusCode::BAD_REQUEST,
    _ => StatusCode::INTERNAL_SERVER_ERROR,
});

impl_serialize_pretty_error!(StorageError);

#[derive(Debug, Error)]
#[error("invalid node target string")]
pub struct NodeTargetError;

impl_into_status_code!(NodeTargetError, |_| StatusCode::BAD_REQUEST);

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

impl_serialize_pretty_error!(KeySourceError);

#[derive(Debug, Error, AsRefStr)]
pub enum SchemaError {
    #[error("key source error: {0}")]
    KeySource(#[from] KeySourceError),
    #[error("node target error: {0}")]
    NodeTarget(#[from] NodeTargetError),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
}

impl_into_status_code!(SchemaError, |value| match value {
    KeySource(e) => e.into(),
    NodeTarget(e) => e.into(),
    Storage(e) => e.into(),
});

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
