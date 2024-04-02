use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use thiserror::Error;

#[derive(Debug, Serialize)]
pub struct PrettyError {
    #[serde(rename = "type")]
    pub type_: String,
    pub error: String,
}

impl<E> From<&E> for PrettyError
where
    E: std::error::Error + AsRef<str>,
{
    fn from(error: &E) -> Self {
        Self {
            type_: error.as_ref().to_string(),
            error: error.to_string(),
        }
    }
}

#[derive(Debug, Error, Serialize, Deserialize, AsRefStr)]
pub enum AgentError {
    #[error("invalid agent state")]
    InvalidState,
    #[error("failed to parse json")]
    FailedToParseJson,
    #[error("failed to make a request")]
    FailedToMakeRequest,
    #[error("failed to spawn a process")]
    FailedToSpawnProcess,
    #[error("process failed")]
    ProcessFailed,
}

#[derive(Debug, Error, Serialize, Deserialize, AsRefStr)]
pub enum ResolveError {
    #[error("source agent not found")]
    SourceAgentNotFound,
    #[error("agent has no addresses")]
    AgentHasNoAddresses,
}

#[derive(Debug, Error, Serialize, Deserialize, AsRefStr)]
pub enum ReconcileError {
    #[error("aborted by a more recent reconcilation request")]
    Aborted,
    #[error("failed to download the specified storage")]
    StorageAcquireError,
    #[error("failed to resolve addresses of stated peers")]
    ResolveAddrError(ResolveError),
    #[error("agent did not provide a local private key")]
    NoLocalPrivateKey,
    #[error("unknown error")]
    Unknown,
}
