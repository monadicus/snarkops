use snot_common::state::{AgentId, NodeKey};
use thiserror::Error;
use tokio::task::JoinError;

use crate::{cannon::error::CannonError, schema::error::SchemaError};

#[derive(Debug, Error)]
#[error("batch reconciliation failed with `{failures}` failed reconciliations")]
pub struct BatchReconcileError {
    pub failures: usize,
}

#[derive(Debug, Error)]
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

#[derive(Debug, Error)]
#[error("deserialize error: `{i}`: `{e}`")]
pub struct DeserializeError {
    pub i: usize,
    #[source]
    pub e: serde_yaml::Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Error, serde::Serialize)]
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

#[derive(Debug, Error)]
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

#[derive(Debug, Error)]
pub enum CleanupError {
    #[error("env `{0}` not found")]
    EnvNotFound(usize),
}

#[derive(Debug, Error)]
pub enum ReconcileError {
    #[error(transparent)]
    Batch(#[from] BatchReconcileError),
    #[error("env `{0}` not found")]
    EnvNotFound(usize),
    #[error("expected internal agent peer for node with key {key}")]
    ExpectedInternalAgentPeer { key: NodeKey },
}

#[derive(Debug, Error)]
pub enum EnvError {
    #[error("cleanup error: `{0}`")]
    Cleanup(#[from] CleanupError),
    #[error("delegation errors occured:\n{}", serde_json::to_string_pretty(&.0).unwrap())]
    Delegation(Vec<DelegationError>),
    #[error("exec error: `{0}`")]
    Execution(#[from] ExecutionError),
    #[error("prepare error: `{0}`")]
    Prepare(#[from] PrepareError),
    #[error("reconcile error: `{0}`")]
    Reconcile(#[from] ReconcileError),
    #[error("schema error: `{0}`")]
    Schema(#[from] SchemaError),
    #[error("cannon error: `{0}`")]
    Cannon(#[from] CannonError),
}
