use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use thiserror::Error;

use crate::state::EnvId;

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
    #[error("failed setup storage: {0}")]
    StorageSetupError(String),
    #[error("failed to download {0} from the control plane")]
    StorageAcquireError(String),
    #[error("failed to get the binary from the control plane: {0}")]
    BinaryAcquireError(String),
    #[error("failed to find a checkpoint for the requested height/span")]
    CheckpointAcquireError,
    #[error("failed to apply checkpoint: {0}")]
    CheckpointApplyError(String),
    #[error("failed to resolve addresses of stated peers")]
    ResolveAddrError(ResolveError),
    #[error("a rention policy is required to rewind the ledger")]
    MissingRetentionPolicy,
    #[error("failed to load checkpoints for storage")]
    CheckpointLoadError,
    #[error("agent did not provide a local private key")]
    NoLocalPrivateKey,
    #[error("generic database error")]
    Database,
    #[error("unknown error")]
    Unknown,
}

#[derive(Debug, Error, Serialize, Deserialize, AsRefStr)]
pub enum ReconcileError2 {
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
}
