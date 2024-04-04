use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use thiserror::Error;

#[macro_export]
macro_rules! impl_serialize_pretty_error {
    ($name:path) => {
        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                PrettyError::from(self).serialize(serializer)
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

#[derive(Debug, Serialize)]
pub struct PrettyError {
    #[serde(rename = "type")]
    pub ty: String,
    pub error: String,
}

impl<E> From<&E> for PrettyError
where
    E: std::error::Error + AsRef<str>,
{
    fn from(error: &E) -> Self {
        Self {
            ty: error.as_ref().to_string(),
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
