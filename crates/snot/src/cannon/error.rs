use std::path::PathBuf;

use snot_common::state::NodeKey;
use thiserror::Error;

use super::Authorization;
use crate::error::CommandError;

#[derive(Debug, Error)]
pub enum AuthorizeError {
    #[error("command error: {0}")]
    Command(#[from] CommandError),
    #[error("expected function, fee, and broadcast fields in response")]
    InvalidJson,
    #[error("parse json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("expected JSON object in response")]
    JsonNotObject,
}

#[derive(Debug, Error)]
pub enum TransactionDrainError {
    #[error("error locking tx drain")]
    FailedToLock,
    #[error("error opening tx drain source file: {0:#?}")]
    FailedToOpenSource(PathBuf),
    #[error("error reading line from tx drain")]
    FailedToReadLine(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum TransactionSinkError {
    #[error("error locking tx drain")]
    FailedToLock,
    #[error("error opening tx sink source file: {0:#?}")]
    FailedToOpenSource(PathBuf),
    #[error("error writing to tx sink: {0}")]
    FailedToWrite(#[from] std::io::Error),
}

#[derive(Debug, Error)]
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
    StateRootInvalidJson(#[from] reqwest::Error),
    #[error("could not get an available port")]
    TxSouceUnavailablePort,
}

// TODO a lot of these could be split into the above.
// Then in mod.rs could we can use the above errors to simplify
// the errors
#[derive(Debug, Error)]
pub enum CannonError {
    #[error("authorize error: {0}")]
    Authorize(#[from] AuthorizeError),
    #[error("broadcast error for exec ctx `{0}`: {1}")]
    Broadcast(usize, String),
    #[error("broadcast error for exec ctx `{0}`: {1}")]
    BroadcastRequest(usize, #[source] reqwest::Error),
    #[error("command error cannon `{0}`: {1}")]
    Command(usize, #[source] CommandError),
    #[error("cannon `{0}` is not configured to playback txs")]
    ConfiguredToPlayback(usize),
    #[
			error("env dropped{}{}`", 
			.0.map(|id| format!(" for cannon `{id}`")).unwrap_or_default(),
			.1.map(|id| format!(" for exec ctx `{id}`")).unwrap_or_default()
		)]
    EnvDropped(Option<usize>, Option<usize>),
    #[error("missing query port for cannon `{0}`")]
    MissingQueryPort(usize),
    #[error("no available agents `{0}` for exec ctx `{1}`")]
    NoAvailableAgents(&'static str, usize),
    #[error("no --host configured for demox based cannon")]
    NoDemoxHostConfigured,
    #[error("no target agent found for cannon `{0}`: {1}")]
    TargetAgentNotFound(usize, NodeKey),
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
    #[error("tx drain `{2}` not found for exec ctx `{0}` for cannon `{1}`")]
    TransactionDrainNotFound(usize, usize, String),
    #[error("tx sink `{2}` not found for exec ctx `{0}` for cannon `{1}`")]
    TransactionSinkNotFound(usize, usize, String),
}
