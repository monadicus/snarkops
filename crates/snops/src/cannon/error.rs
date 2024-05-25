use std::path::PathBuf;

use axum::http::StatusCode;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use snops_common::{
    aot_cmds::{error::CommandError, AotCmdError},
    impl_into_status_code, impl_into_type_str,
    state::{CannonId, EnvId, NodeKey, TxPipeId},
};
use strum_macros::AsRefStr;
use thiserror::Error;

use super::Authorization;
use crate::error::StateError;

#[derive(Debug, Error, AsRefStr)]
pub enum AuthorizeError {
    /// For when a bad AOT command is run
    #[error(transparent)]
    Command(#[from] AotCmdError),
    /// For if invalid JSON is returned from the AOT command
    #[error("{0}")]
    Json(#[source] serde_json::Error),
    #[error("program {0} has invalid inputs {1}")]
    InvalidProgramInputs(String, String),
    #[error("execution {0} requires a valid private key: {1}")]
    MissingPrivateKey(String, String),
}

impl_into_status_code!(AuthorizeError, |value| match value {
    Command(e) => e.into(),
    _ => StatusCode::INTERNAL_SERVER_ERROR,
});

impl_into_type_str!(AuthorizeError, |value| match value {
    Command(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    _ => value.as_ref().to_string(),
});

impl Serialize for AuthorizeError {
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

#[derive(Debug, Error, AsRefStr)]
pub enum TransactionDrainError {
    /// For when the tx drain cannot be locked
    #[error("error locking tx drain")]
    FailedToLock,
    /// For when the tx drain source file cannot be opened
    #[error("error opening tx drain source file: {0:#?}")]
    FailedToOpenSource(PathBuf, #[source] std::io::Error),
    /// For when a line cannot be read from the tx drain file
    #[error("error reading line from tx drain: {0}")]
    FailedToReadLine(#[source] std::io::Error),
}

impl_into_status_code!(TransactionDrainError);

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

impl_into_status_code!(TransactionSinkError);

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
    TxSourceUnavailablePort,
}

impl_into_status_code!(SourceError);

#[derive(Debug, Error, AsRefStr)]
pub enum CannonInstanceError {
    #[error("missing query port for cannon `{0}`")]
    MissingQueryPort(CannonId),
    #[error("cannon `{0}` is not configured to playback txs")]
    NotConfiguredToPlayback(CannonId),
    #[error("no target agent found for cannon `{0}`: {1}")]
    TargetAgentNotFound(CannonId, NodeKey),
}

impl_into_status_code!(CannonInstanceError, |value| match value {
    MissingQueryPort(_) | NotConfiguredToPlayback(_) => StatusCode::BAD_REQUEST,
    TargetAgentNotFound(_, _) => StatusCode::NOT_FOUND,
});

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
    Broadcast(CannonId, String),
    #[error("broadcast error for exec ctx `{0}`: {1}")]
    BroadcastRequest(CannonId, #[source] reqwest::Error),
    #[error("env {0} dropped for cannon {1}`")]
    EnvDropped(EnvId, CannonId),
    #[error("no available agents `{0}` for exec ctx `{1}`")]
    NoAvailableAgents(&'static str, CannonId),
    #[error("no --hostname configured for demox based cannon")]
    NoHostnameConfigured,
    #[error("tx drain `{2}` not found for exec ctx `{0}` for cannon `{1}`")]
    TransactionDrainNotFound(EnvId, CannonId, TxPipeId),
    #[error("tx sink `{2}` not found for exec ctx `{0}` for cannon `{1}`")]
    TransactionSinkNotFound(EnvId, CannonId, TxPipeId),
}

impl_into_status_code!(ExecutionContextError, |value| match value {
    Broadcast(_, _) | BroadcastRequest(_, _) => StatusCode::MISDIRECTED_REQUEST,
    NoAvailableAgents(_, _) | NoHostnameConfigured => StatusCode::SERVICE_UNAVAILABLE,
    _ => StatusCode::INTERNAL_SERVER_ERROR,
});

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
    #[error(transparent)]
    Authorize(#[from] AuthorizeError),
    #[error(transparent)]
    CannonInstance(#[from] CannonInstanceError),
    #[error("cannon `{0}`: {1}")]
    Command(CannonId, #[source] CommandError),
    #[error(transparent)]
    ExecutionContext(#[from] ExecutionContextError),
    #[error("target agent offline for {0} `{1}`: {2}")]
    TargetAgentOffline(&'static str, CannonId, String),
    #[error(transparent)]
    TransactionDrain(#[from] TransactionDrainError),
    #[error(transparent)]
    TransactionSink(#[from] TransactionSinkError),
    #[error("send `auth` error for cannon `{0}`: {1}")]
    SendAuthError(
        CannonId,
        #[source] tokio::sync::mpsc::error::SendError<Authorization>,
    ),
    #[error("send `tx` error for cannon `{0}`: {1}")]
    SendTxError(
        CannonId,
        #[source] tokio::sync::mpsc::error::SendError<String>,
    ),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    State(#[from] StateError),
}

impl_into_status_code!(CannonError, |value| match value {
    Authorize(e) => e.into(),
    CannonInstance(e) => e.into(),
    Command(_, e) => e.into(),
    ExecutionContext(e) => e.into(),
    TargetAgentOffline(_, _, _) => StatusCode::SERVICE_UNAVAILABLE,
    TransactionDrain(e) => e.into(),
    TransactionSink(e) => e.into(),
    Source(e) => e.into(),
    State(e) => e.into(),
    _ => StatusCode::INTERNAL_SERVER_ERROR,
});

impl_into_type_str!(CannonError, |value| match value {
    Authorize(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    CannonInstance(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    Command(_, e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    ExecutionContext(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    TransactionDrain(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    TransactionSink(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    Source(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    State(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    _ => value.as_ref().to_string(),
});

impl Serialize for CannonError {
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
