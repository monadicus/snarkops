use std::process::ExitStatus;

use serde::{ser::SerializeStruct, Serialize, Serializer};
use snot_common::rpc::error::PrettyError;
use snot_common::state::AgentId;
use snot_common::{impl_into_status_code, impl_serialize_pretty_error};
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

impl_into_status_code!(CommandError);
impl_serialize_pretty_error!(CommandError);

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

impl_into_status_code!(DeserializeError);

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

impl_into_status_code!(StateError);

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
