use std::path::PathBuf;

use axum::http::StatusCode;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use snops_common::{
    aot_cmds::error::CommandError,
    impl_into_status_code, impl_into_type_str,
    key_source::KeySourceError,
    node_targets::NodeTargetError,
    state::{InternedId, StorageId},
};
use strum_macros::AsRefStr;
use thiserror::Error;
use url::Url;

#[derive(Debug, Error, AsRefStr)]
pub enum StorageError {
    #[error("storage id: `{1}`: {0}")]
    Command(CommandError, StorageId),
    #[error("invalid storage id {0}")]
    InvalidStorageId(String),
    #[error("mkdirs for storage generation id: `{0}`: {1}")]
    GenerateStorage(StorageId, #[source] std::io::Error),
    #[error("remove storage {0:#?}: {1}")]
    RemoveStorage(PathBuf, #[source] std::io::Error),
    #[error("generating genesis id: `{0}`: {1}")]
    FailedToGenGenesis(StorageId, #[source] std::io::Error),
    #[error("fetching genesis block id: `{0}` url: `{1}`: {2}")]
    FailedToFetchGenesis(StorageId, Url, #[source] reqwest::Error),
    #[error("writing genesis block id: `{0}`: {1}")]
    FailedToWriteGenesis(StorageId, #[source] std::io::Error),
    #[error("creating ledger dir id: `{0}`: {1}")]
    FailedToCreateLedgerDir(StorageId, #[source] std::io::Error),
    #[error("taring ledger id: `{0}`: {1}")]
    FailedToTarLedger(StorageId, #[source] std::io::Error),
    #[error("the specified storage ID {0} doesn't exist, and no generation params were specified")]
    NoGenerationParams(StorageId),
    #[error("reading balances {0:#?}: {1}")]
    ReadBalances(PathBuf, #[source] std::io::Error),
    #[error("reading version {0:#?}: {1}")]
    ReadVersion(PathBuf, #[source] std::io::Error),
    #[error("writing version {0:#?}: {1}")]
    WriteVersion(PathBuf, #[source] std::io::Error),
    #[error("writing commmittee {0:#?}: {1}")]
    WriteCommittee(PathBuf, #[source] std::io::Error),
    #[error("parsing balances {0:#?}: {1}")]
    ParseBalances(PathBuf, #[source] serde_json::Error),
    #[error("error loading checkpoints: {0}")]
    CheckpointManager(#[from] checkpoint::errors::ManagerLoadError),
    #[error("binary with id `{0}` does not exist for storage id: {1}")]
    BinaryDoesNotExist(InternedId, StorageId),
    #[error("failed fetching binary with id `{0}` from url `{1}`: {2}")]
    FailedToFetchBinary(InternedId, Url, #[source] reqwest::Error),
    #[error("failed fetching binary with id `{0}` from url `{1}`: status {2}")]
    FailedToFetchBinaryWithStatus(InternedId, Url, StatusCode),
    #[error("failed to create binary file with id `{0}`: {1}")]
    FailedToCreateBinaryFile(InternedId, #[source] std::io::Error),
    #[error("failed to write binary file with id `{0}`: {1}")]
    FailedToWriteBinaryFile(InternedId, #[source] std::io::Error),
    #[error("missing binary file `{0}`: {1}")]
    BinaryFileMissing(InternedId, PathBuf),
    #[error("binary file `{0}` at path `{1}` has size mismatch: expected {2}, found {3}")]
    BinarySizeMismatch(InternedId, PathBuf, u64, u64),
    #[error("binary file `{0}` at path `{1}` has sha256 mismatch expected `{2}`, found `{3}`")]
    BinarySha256Mismatch(InternedId, PathBuf, String, String),
    #[error("binary file `{0}` at path `{1}` has failed check: {2}")]
    BinaryCheckFailed(InternedId, PathBuf, String),
}

impl_into_status_code!(StorageError, |value| match value {
    Command(e, _) => e.into(),
    FailedToFetchGenesis(_, _, _) => StatusCode::MISDIRECTED_REQUEST,
    NoGenerationParams(_) => StatusCode::BAD_REQUEST,
    BinaryDoesNotExist(_, _) => StatusCode::NOT_FOUND,
    BinaryFileMissing(_, _) => StatusCode::NOT_FOUND,
    _ => StatusCode::INTERNAL_SERVER_ERROR,
});

impl_into_type_str!(StorageError, |value| match value {
    Command(e, _) => format!("{}.{}", value.as_ref(), e.as_ref()),
    _ => value.as_ref().to_string(),
});

#[derive(Debug, Error, AsRefStr)]
pub enum SchemaError {
    #[error("key source error: {0}")]
    KeySource(#[from] KeySourceError),
    #[error(transparent)]
    NodeTarget(#[from] NodeTargetError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("query parse error: {0}")]
    QueryParse(String),
}

impl_into_status_code!(SchemaError, |value| match value {
    KeySource(e) => e.into(),
    NodeTarget(e) => e.into(),
    Storage(e) => e.into(),
    QueryParse(_) => StatusCode::BAD_REQUEST,
});

impl_into_type_str!(SchemaError, |value| match value {
    KeySource(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    Storage(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    QueryParse(e) => format!("{}.{}", value.as_ref(), e),
    _ => value.as_ref().to_string(),
});

impl Serialize for SchemaError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", &String::from(self))?;
        state.serialize_field("error", &self.to_string())?;

        state.end()
    }
}
