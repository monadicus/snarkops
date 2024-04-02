use axum::http::StatusCode;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use snot_common::{
    rpc::error::PrettyError,
    state::{AgentId, NodeKey},
};
use strum_macros::AsRefStr;
use thiserror::Error;
use tokio::task::JoinError;

use crate::{cannon::error::CannonError, schema::error::SchemaError};

#[derive(Debug, Error)]
#[error("batch reconciliation failed with `{failures}` failed reconciliations")]
pub struct BatchReconcileError {
    pub failures: usize,
}

impl BatchReconcileError {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum ExecutionError {
    #[error("an agent is offline, so the test cannot complete")]
    AgentOffline,
    #[error("env `{0}` not found")]
    EnvNotFound(usize),
    #[error("cannon error: `{0}`")]
    Cannon(#[from] CannonError),
    #[error("join error: `{0}`")]
    Join(#[from] JoinError),
    #[error("reconcile error: `{0}`")]
    Reconcile(#[from] BatchReconcileError),
    #[error("env timeline is already being executed")]
    TimelineAlreadyStarted,
    #[error("unknown cannon: `{0}`")]
    UnknownCannon(String),
}

impl ExecutionError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Cannon(e) => e.status_code(),
            Self::Reconcile(e) => e.status_code(),
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl Serialize for ExecutionError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", self.as_ref())?;

        match self {
            Self::Cannon(e) => state.serialize_field("error", e),
            _ => state.serialize_field("error", &self.to_string()),
        }?;

        state.end()
    }
}

// TODO move to a more common error and re-use?
#[derive(Debug, Error)]
#[error("deserialize error: `{i}`: `{e}`")]
pub struct DeserializeError {
    pub i: usize,
    #[source]
    pub e: serde_yaml::Error,
}

impl DeserializeError {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error, AsRefStr)]
pub enum DelegationError {
    #[error("agent {0} already claimed for node {1}")]
    AgentAlreadyClaimed(AgentId, NodeKey),
    #[error("agent {0} does not support the mode needed for {1}")]
    AgentMissingMode(AgentId, NodeKey),
    #[error("agent {0} not found for node {1}")]
    AgentNotFound(AgentId, NodeKey),
    #[error("insufficient number of agents to satisfy the request")]
    InsufficientAgentCount,
    #[error("could not find any agents for node {0}")]
    NoAvailableAgents(NodeKey),
}

impl DelegationError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::AgentAlreadyClaimed(_, _) => StatusCode::IM_USED,
            Self::AgentNotFound(_, _) => StatusCode::NOT_FOUND,
            Self::AgentMissingMode(_, _) => StatusCode::BAD_REQUEST,
            Self::InsufficientAgentCount | Self::NoAvailableAgents(_) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
        }
    }
}

impl Serialize for DelegationError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PrettyError::from(self).serialize(serializer)
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum PrepareError {
    #[error("duplicate node key: {0}")]
    DuplicateNodeKey(NodeKey),
    #[error("multiple storage documents found in env")]
    MultipleStorage,
    #[error("missing storage document in env")]
    MissingStorage,
    #[error("cannot have a node with zero replicas")]
    NodeHas0Replicas,
    #[error("reconcile error: `{0}`")]
    Reconcile(#[from] ReconcileError),
}

impl PrepareError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::DuplicateNodeKey(_) | Self::MultipleStorage | Self::NodeHas0Replicas => {
                StatusCode::BAD_REQUEST
            }
            Self::MissingStorage => StatusCode::NOT_FOUND,
            Self::Reconcile(e) => e.status_code(),
        }
    }
}

impl Serialize for PrepareError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", self.as_ref())?;

        match self {
            Self::Reconcile(e) => state.serialize_field("error", e),
            _ => state.serialize_field("error", &self.to_string()),
        }?;

        state.end()
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum CleanupError {
    #[error("env `{0}` not found")]
    EnvNotFound(usize),
}

impl CleanupError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::EnvNotFound(_) => StatusCode::NOT_FOUND,
        }
    }
}

impl Serialize for CleanupError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PrettyError::from(self).serialize(serializer)
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum ReconcileError {
    #[error(transparent)]
    Batch(#[from] BatchReconcileError),
    #[error("env `{0}` not found")]
    EnvNotFound(usize),
    #[error("expected internal agent peer for node with key {key}")]
    ExpectedInternalAgentPeer { key: NodeKey },
}

impl ReconcileError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Batch(e) => e.status_code(),
            Self::EnvNotFound(_) | Self::ExpectedInternalAgentPeer { .. } => StatusCode::NOT_FOUND,
        }
    }
}

impl Serialize for ReconcileError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PrettyError::from(self).serialize(serializer)
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum EnvError {
    #[error("cannon error: `{0}`")]
    Cannon(#[from] CannonError),
    #[error("cleanup error: `{0}`")]
    Cleanup(#[from] CleanupError),
    #[error("delegation errors occured:{}", .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"))]
    Delegation(Vec<DelegationError>),
    #[error("exec error: `{0}`")]
    Execution(#[from] ExecutionError),
    #[error("prepare error: `{0}`")]
    Prepare(#[from] PrepareError),
    #[error("reconcile error: `{0}`")]
    Reconcile(#[from] ReconcileError),
    #[error("schema error: `{0}`")]
    Schema(#[from] SchemaError),
}

impl EnvError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Cannon(e) => e.status_code(),
            Self::Cleanup(e) => e.status_code(),
            Self::Delegation(e) => e
                .iter()
                .fold(StatusCode::OK, |acc, x| acc.max(x.status_code())),
            Self::Execution(e) => e.status_code(),
            Self::Prepare(e) => e.status_code(),
            Self::Reconcile(e) => e.status_code(),
            Self::Schema(e) => e.status_code(),
        }
    }
}

impl Serialize for EnvError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", self.as_ref())?;

        match self {
            Self::Cannon(e) => state.serialize_field("error", e),
            Self::Cleanup(e) => state.serialize_field("error", e),
            Self::Delegation(e) => state.serialize_field("error", e),
            Self::Execution(e) => state.serialize_field("error", e),
            Self::Prepare(e) => state.serialize_field("error", e),
            Self::Reconcile(e) => state.serialize_field("error", e),
            Self::Schema(e) => state.serialize_field("error", e),
        }?;

        state.end()
    }
}
