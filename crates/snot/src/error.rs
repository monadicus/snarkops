use std::process::ExitStatus;

use snot_common::state::AgentId;
use thiserror::Error;

#[derive(Debug, Error)]
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
pub enum StateError {
    #[error("common agent error: {0}")]
    Agent(#[from] snot_common::prelude::agent::AgentError),
    #[error("source agent has no addr id: `{0}`")]
    NoAddress(AgentId),
    #[error("common reconcile error: {0}")]
    Reconcile(#[from] snot_common::prelude::agent::ReconcileError),
    #[error("rpc error: {0}")]
    Rpc(#[from] tarpc::client::RpcError),
    #[error("source agent not found id: `{0}`")]
    SourceAgentNotFound(AgentId),
}
