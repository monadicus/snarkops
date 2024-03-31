use std::process::ExitStatus;

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
