use std::path::PathBuf;

use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
#[error("error {action} command `{cmd}`: {error}")]
pub struct CommandError {
    pub action: &'static str,
    pub cmd: &'static str,
    #[source]
    pub error: std::io::Error,
}

impl CommandError {
    pub fn new(action: &'static str, cmd: &'static str, error: std::io::Error) -> Self {
        Self { action, cmd, error }
    }
}

#[derive(Debug, Error)]
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

#[derive(Debug, Error)]
#[error("invalid node target string")]
pub struct NodeTargetError;

#[derive(Debug, Error)]
pub enum KeySourceError {
    #[error("invalid key source string")]
    InvalidKeySource,
    #[error("invalid committee index: {0}")]
    InvalidCommitteeIndex(#[source] std::num::ParseIntError),
}

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("key source error: {0}")]
    KeySource(#[from] KeySourceError),
    #[error("node target error: {0}")]
    NodeTarget(#[from] NodeTargetError),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
}
