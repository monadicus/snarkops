use std::path::PathBuf;

use axum::http::StatusCode;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use snot_common::{rpc::error::PrettyError, state::NodeKey};
use strum_macros::AsRefStr;
use thiserror::Error;

use super::Authorization;
use crate::error::{CommandError, StateError};

#[derive(Debug, Error, AsRefStr)]
pub enum AuthorizeError {
    /// For when a bad AOT command is run
    #[error("command error: {0}")]
    Command(#[from] CommandError),
    /// For if invalid JSON is returned from the AOT command
    #[error("expected function, fee, and broadcast fields in response")]
    InvalidJson,
    /// For if invalid JSON is returned from the AOT command
    #[error("parse json error: {0}")]
    Json(#[source] serde_json::Error),
    /// For if invalid JSON is returned from the AOT command
    #[error("expected JSON object in response")]
    JsonNotObject,
}

impl AuthorizeError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Command(e) => e.status_code(),
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl Serialize for AuthorizeError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", self.as_ref())?;

        match self {
            Self::Command(e) => state.serialize_field("error", e),
            _ => state.serialize_field("error", &self.to_string()),
        }?;

        state.end()
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum TransactionDrainError {
    /// For when the tx drain cannot be locked
    #[error("error locking tx drain")]
    FailedToLock,
    /// For when the tx drain source file cannot be opened
    #[error("error opening tx drain source file: {0:#?}")]
    FailedToOpenSource(PathBuf),
    /// For when a line cannot be read from the tx drain file
    #[error("error reading line from tx drain: {0}")]
    FailedToReadLine(#[source] std::io::Error),
}

impl TransactionDrainError {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

impl Serialize for TransactionDrainError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PrettyError::from(self).serialize(serializer)
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum TransactionSinkError {
    /// For when the tx sink cannot be locked
    #[error("error locking tx sink")]
    FailedToLock,
    /// For when the tx sink source file cannot be opened
    #[error("error opening tx sink source file: {0:#?}")]
    FailedToOpenSource(PathBuf),
    /// For when a line cannot be written to the tx sink file
    #[error("error writing to tx sink: {0}")]
    FailedToWrite(#[source] std::io::Error),
}

impl TransactionSinkError {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

impl Serialize for TransactionSinkError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PrettyError::from(self).serialize(serializer)
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum SourceError {
    #[error("cannot authorize playback txs")]
    CannotAuthorizePlaybackTx,
    #[error("error selecting a valid `{0}`")]
    CouldNotSelect(&'static str),
    #[error("error fetching state root from `{0}`: {1}")]
    FailedToGetStateRoot(String, #[source] reqwest::Error),
    #[error("error jsonifying `{0}`: {1}")]
    Json(&'static str, #[source] serde_json::Error),
    #[error("no agents available to execute `{0}`")]
    NoAvailableAgents(&'static str),
    #[error("no tx modes available for this cannon instance??")]
    NoTxModeAvailable,
    #[error("error parsing state root JSON: {0}")]
    StateRootInvalidJson(#[source] reqwest::Error),
    #[error("could not get an available port")]
    TxSouceUnavailablePort,
}

impl SourceError {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

impl Serialize for SourceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PrettyError::from(self).serialize(serializer)
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum CannonInstanceError {
    #[error("missing query port for cannon `{0}`")]
    MissingQueryPort(usize),
    #[error("cannon `{0}` is not configured to playback txs")]
    NotConfiguredToPlayback(usize),
    #[error("no target agent found for cannon `{0}`: {1}")]
    TargetAgentNotFound(usize, NodeKey),
}

impl CannonInstanceError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::MissingQueryPort(_) | Self::NotConfiguredToPlayback(_) => StatusCode::BAD_REQUEST,
            Self::TargetAgentNotFound(_, _) => StatusCode::NOT_FOUND,
        }
    }
}

impl Serialize for CannonInstanceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", self.as_ref())?;
        state.serialize_field("error", &self.to_string())?;

        state.end()
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum ExecutionContextError {
    #[error("broadcast error for exec ctx `{0}`: {1}")]
    Broadcast(usize, String),
    #[error("broadcast error for exec ctx `{0}`: {1}")]
    BroadcastRequest(usize, #[source] reqwest::Error),
    #[
			error("env dropped{}{}`", 
			.0.map(|id| format!(" for cannon `{id}`")).unwrap_or_default(),
			.1.map(|id| format!(" for exec ctx `{id}`")).unwrap_or_default()
		)]
    EnvDropped(Option<usize>, Option<usize>),
    #[error("no available agents `{0}` for exec ctx `{1}`")]
    NoAvailableAgents(&'static str, usize),
    #[error("no --host configured for demox based cannon")]
    NoDemoxHostConfigured,
    #[error("tx drain `{2}` not found for exec ctx `{0}` for cannon `{1}`")]
    TransactionDrainNotFound(usize, usize, String),
    #[error("tx sink `{2}` not found for exec ctx `{0}` for cannon `{1}`")]
    TransactionSinkNotFound(usize, usize, String),
}

impl ExecutionContextError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Broadcast(_, _) | Self::BroadcastRequest(_, _) => StatusCode::MISDIRECTED_REQUEST,
            Self::NoAvailableAgents(_, _) | Self::NoDemoxHostConfigured => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl Serialize for ExecutionContextError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", self.as_ref())?;
        state.serialize_field("error", &self.to_string())?;

        state.end()
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum CannonError {
    #[error("authorize error: {0}")]
    Authorize(#[from] AuthorizeError),
    #[error("cannon instance error: {0}")]
    CannonInstance(#[from] CannonInstanceError),
    #[error("command error cannon `{0}`: {1}")]
    Command(usize, #[source] CommandError),
    #[error("exec ctx error: {0}")]
    ExecutionContext(#[from] ExecutionContextError),
    #[error("target agent offline for {0} `{1}`: {2}")]
    TargetAgentOffline(&'static str, usize, String),
    #[error("tx drain error: {0}")]
    TransactionDrain(#[from] TransactionDrainError),
    #[error("tx sink error: {0}")]
    TransactionSink(#[from] TransactionSinkError),
    #[error("send `auth` error for cannon `{0}`: {1}")]
    SendAuthError(
        usize,
        #[source] tokio::sync::mpsc::error::SendError<Authorization>,
    ),
    #[error("send `tx` error for cannon `{0}`: {1}")]
    SendTxError(usize, #[source] tokio::sync::mpsc::error::SendError<String>),
    #[error("source error: {0}")]
    Source(#[from] SourceError),
    #[error("state error: {0}")]
    State(#[from] StateError),
}

impl CannonError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Authorize(e) => e.status_code(),
            Self::CannonInstance(e) => e.status_code(),
            Self::Command(_, e) => e.status_code(),
            Self::ExecutionContext(e) => e.status_code(),
            Self::TargetAgentOffline(_, _, _) => StatusCode::SERVICE_UNAVAILABLE,
            Self::TransactionDrain(e) => e.status_code(),
            Self::TransactionSink(e) => e.status_code(),
            Self::Source(e) => e.status_code(),
            Self::State(e) => e.status_code(),
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl Serialize for CannonError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", self.as_ref())?;

        match self {
            Self::Authorize(e) => state.serialize_field("error", e),
            Self::CannonInstance(e) => state.serialize_field("error", e),
            Self::Command(_, e) => state.serialize_field("error", e),
            Self::ExecutionContext(e) => state.serialize_field("error", e),
            Self::TransactionDrain(e) => state.serialize_field("error", e),
            Self::TransactionSink(e) => state.serialize_field("error", e),
            _ => state.serialize_field("error", &self.to_string()),
        }?;

        state.end()
    }
}
