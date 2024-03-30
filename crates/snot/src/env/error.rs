use snot_common::state::NodeKey;
use thiserror::Error;
use tokio::task::JoinError;

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
    Cannon(anyhow::Error),
}

#[derive(Debug, Error)]
#[error("deserialize error: `{i}`: `{e}`")]
pub struct DeserializeError {
    pub i: usize,
    #[source]
    pub e: serde_yaml::Error,
}

#[derive(Debug, Error)]
pub enum PrepareError {
    #[error("duplicate node key: {0}")]
    DuplicateNodeKey(NodeKey),
    #[error("cannot have a node with zero replicas")]
    NodeHas0Replicas,
    #[error(
        "not enough available agents to satisfy node topology: needs {0}, but only {1} available"
    )]
    NotEnoughAvailableNodes(usize, usize),
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
    #[error("exec error: `{0}`")]
    Execution(#[from] ExecutionError),
    #[error("prepare error: `{0}`")]
    Prepare(#[from] PrepareError),
    #[error("reconcile error: `{0}`")]
    Reconcile(#[from] ReconcileError),
}
