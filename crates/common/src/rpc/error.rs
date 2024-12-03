use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use thiserror::Error;

use crate::state::{EnvId, HeightRequest};

#[macro_export]
macro_rules! impl_into_type_str {
    ($name:path) => {
        impl From<&$name> for String {
            fn from(e: &$name) -> Self {
                e.as_ref().to_string()
            }
        }
    };

    ($name:path, |_| $body:expr) => {
        impl From<&$name> for String {
            fn from(_: &$name) -> Self {
                $body
            }
        }
    };

    ($name:path, |$from_var:ident| $body:expr) => {
        impl From<&$name> for String {
            fn from($from_var: &$name) -> Self {
                use $name::*;

                $body
            }
        }
    };
}

#[macro_export]
macro_rules! impl_into_status_code {
    ($name:path) => {
        impl From<&$name> for ::http::status::StatusCode {
            fn from(_: &$name) -> Self {
                Self::INTERNAL_SERVER_ERROR
            }
        }
    };

    ($name:path, |_| $body:expr) => {
        impl From<&$name> for ::http::status::StatusCode {
            fn from(_: &$name) -> Self {
                $body
            }
        }
    };

    ($name:path, |$from_var:ident| $body:expr) => {
        impl From<&$name> for ::http::status::StatusCode {
            fn from($from_var: &$name) -> Self {
                use $name::*;

                $body
            }
        }
    };
}

#[derive(Debug, Error, Serialize, Deserialize, AsRefStr)]
pub enum AgentError {
    #[error("invalid agent state")]
    InvalidState,
    #[error("failed to parse json")]
    FailedToParseJson,
    #[error("failed to make a request")]
    FailedToMakeRequest,
    #[error("failed to get env info: {0}")]
    FailedToGetEnvInfo(String),
    #[error("failed to spawn a process")]
    FailedToSpawnProcess,
    #[error("process failed")]
    ProcessFailed,
    // TODO @gluax move these errors to a new enum
    #[error("invalid log level: `{0}`")]
    InvalidLogLevel(String),
    #[error("failed to change log level")]
    FailedToChangeLogLevel,
    #[error("node client not set")]
    NodeClientNotSet,
    #[error("node client not ready")]
    NodeClientNotReady,
    #[error("invalid block hash")]
    InvalidBlockHash,
    #[error("invalid transaction id")]
    InvalidTransactionId,
}

#[derive(Debug, Error, Serialize, Deserialize, AsRefStr)]
pub enum SnarkosRequestError {
    #[error("expected agent to be in Node state")]
    InvalidState,
    #[error("expected Node to be online")]
    OfflineNode,
    #[error("failed to obtain environment info")]
    MissingEnvInfo,
    #[error("error making request: {0}")]
    RequestError(String),
    #[error("error parsing json: {0}")]
    JsonParseError(String),
    #[error("error serializing json: {0}")]
    JsonSerializeError(String),
    #[error("error deserializing json: {0}")]
    JsonDeserializeError(String),
    #[error("rpc error: {0}")]
    RpcError(String),
    #[error("request timed out")]
    TimedOut,
}

#[derive(Debug, Clone, Error, Serialize, Deserialize, AsRefStr)]
pub enum ResolveError {
    #[error("source agent not found")]
    SourceAgentNotFound,
    #[error("agent has no addresses")]
    AgentHasNoAddresses,
}

#[derive(Debug, Clone, Error, Serialize, Deserialize, AsRefStr)]
#[serde(tag = "error", content = "message")]
pub enum ReconcileError {
    #[error("node is not connected to the controlplane")]
    Offline,
    #[error("env {0} not found")]
    MissingEnv(EnvId),
    #[error("unknown error")]
    Unknown,
    #[error("rpc error: {0}")]
    RpcError(String),
    #[error(transparent)]
    AddressResolve(#[from] ResolveError),
    #[error("missing local private key")]
    MissingLocalPrivateKey,
    #[error("failed to create directory {0}: {1}")]
    CreateDirectory(PathBuf, String),
    #[error("failed to delete file {0}: {1}")]
    DeleteFileError(PathBuf, String),
    #[error("failed to get metadata for {0}: {1}")]
    FileStatError(PathBuf, String),
    #[error("failed to read file {0}: {1}")]
    FileReadError(PathBuf, String),
    #[error("failed to make {method} request {url}: {error}")]
    HttpError {
        method: String,
        url: String,
        error: String,
    },
    #[error("failed to spawn process: {0}")]
    SpawnError(String),
    #[error("failed to set file permissions {0}: {1}")]
    FilePermissionError(PathBuf, String),
    #[error("failed to parse {0} as a url: {1}")]
    UrlParseError(String, String),
    #[error("error loading checkpoints: {0}")]
    CheckpointLoadError(String),
    #[error("missing retention policy for request: {0}")]
    MissingRetentionPolicy(HeightRequest),
    #[error("no available checkpoints for request: {0}")]
    NoAvailableCheckpoints(HeightRequest),
    #[error("failed to apply checkpoint: {0}")]
    CheckpointApplyError(String),
}
