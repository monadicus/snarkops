use std::process::ExitStatus;

use axum::http::StatusCode;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use snot_common::rpc::error::PrettyError;
use snot_common::state::AgentId;
use strum_macros::AsRefStr;
use thiserror::Error;

#[derive(Debug, Error, AsRefStr)]
pub enum CommandError {
    #[error("error {action} command `{cmd}`: {error}")]
    Action {
        action: &'static str,
        cmd: &'static str,
        #[source]
        error: std::io::Error,
    },
    #[error("error command `{cmd}` failed with `{status}`: {stderr}")]
    Status {
        cmd: &'static str,
        status: ExitStatus,
        stderr: String,
    },
}

impl CommandError {
    pub fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

impl Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PrettyError::from(self).serialize(serializer)
    }
}

impl CommandError {
    pub fn action(action: &'static str, cmd: &'static str, error: std::io::Error) -> Self {
        Self::Action { action, cmd, error }
    }

    pub(crate) fn status(cmd: &'static str, status: ExitStatus, stderr: String) -> CommandError {
        Self::Status {
            cmd,
            status,
            stderr,
        }
    }
}

#[derive(Debug, Error)]
#[error("deserialize error: `{i}`: `{e}`")]
pub struct DeserializeError {
    pub i: usize,
    #[source]
    pub e: serde_yaml::Error,
}

impl DeserializeError {
    pub fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum StateError {
    #[error("common agent error: {0}")]
    Agent(#[from] snot_common::prelude::error::AgentError),
    #[error("source agent has no addr id: `{0}`")]
    NoAddress(AgentId),
    #[error("common reconcile error: {0}")]
    Reconcile(#[from] snot_common::prelude::error::ReconcileError),
    #[error("rpc error: {0}")]
    Rpc(#[from] tarpc::client::RpcError),
    #[error("source agent not found id: `{0}`")]
    SourceAgentNotFound(AgentId),
}

impl StateError {
    pub fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

impl Serialize for StateError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", self.as_ref())?;

        match self {
            Self::Agent(e) => {
                let pe = PrettyError::from(e);
                state.serialize_field("error", &pe)
            }
            Self::Reconcile(e) => {
                let pe = PrettyError::from(e);
                state.serialize_field("error", &pe)
            }
            _ => state.serialize_field("error", &self.to_string()),
        }?;

        state.end()
    }
}
