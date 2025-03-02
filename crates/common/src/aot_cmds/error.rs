use std::process::ExitStatus;

use http::StatusCode;
use serde::{Serialize, Serializer, ser::SerializeStruct};
use strum_macros::AsRefStr;
use thiserror::Error;

use crate::{impl_into_status_code, impl_into_type_str};

#[derive(Debug, Error, AsRefStr)]
pub enum CommandError {
    #[error("{action} command `{cmd}`: {error}")]
    Action {
        action: &'static str,
        cmd: &'static str,
        #[source]
        error: std::io::Error,
    },
    #[error("command `{cmd}` failed with `{status}`: {stderr}")]
    Status {
        cmd: &'static str,
        status: ExitStatus,
        stderr: String,
    },
}

impl_into_status_code!(CommandError);

impl CommandError {
    pub fn action(action: &'static str, cmd: &'static str, error: std::io::Error) -> Self {
        Self::Action { action, cmd, error }
    }

    pub fn status(cmd: &'static str, status: ExitStatus, stderr: String) -> CommandError {
        Self::Status {
            cmd,
            status,
            stderr,
        }
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum AotCmdError {
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl_into_status_code!(AotCmdError, |value| match value {
    Command(e) => e.into(),
    Json(_) => StatusCode::UNPROCESSABLE_ENTITY,
});

impl_into_type_str!(AotCmdError, |value| match value {
    Command(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    Json(_) => "json".to_string(),
});

impl Serialize for AotCmdError {
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
