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

#[derive(Debug, Error, AsRefStr)]
pub enum ExecutionError {
    #[error("env `{0}` not found")]
    EnvNotFound(usize),
    #[error("env timeline is already being executed")]
    TimelineAlreadyStarted,
    #[error("an agent is offline, so the test cannot complete")]
    AgentOffline,
    #[error("reconcile error: `{0}`")]
    Reconcile(#[from] BatchReconcileError),
    #[error("join error: `{0}`")]
    Join(#[from] JoinError),
    #[error("unknown cannon: `{0}`")]
    UnknownCannon(String),
    #[error("cannon error: `{0}`")]
    Cannon(#[from] CannonError),
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

#[derive(Debug, Error)]
#[error("deserialize error: `{i}`: `{e}`")]
pub struct DeserializeError {
    pub i: usize,
    #[source]
    pub e: serde_yaml::Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Error, AsRefStr)]
pub enum DelegationError {
    #[error("insufficient number of agents to satisfy the request")]
    InsufficientAgentCount,
    #[error("agent {0} not found for node {1}")]
    AgentNotFound(AgentId, NodeKey),
    #[error("agent {0} already claimed for node {1}")]
    AgentAlreadyClaimed(AgentId, NodeKey),
    #[error("agent {0} does not support the mode needed for {1}")]
    AgentMissingMode(AgentId, NodeKey),
    #[error("could not find any agents for node {0}")]
    NoAvailableAgents(NodeKey),
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
    #[error("cannot have a node with zero replicas")]
    NodeHas0Replicas,
    #[error("multiple storage documents found in env")]
    MultipleStorage,
    #[error("missing storage document in env")]
    MissingStorage,
    #[error("reconcile error: `{0}`")]
    Reconcile(#[from] ReconcileError),
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
