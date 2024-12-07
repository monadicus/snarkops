use axum::http::StatusCode;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use snops_common::{
    aot_cmds::AotCmdError,
    impl_into_status_code, impl_into_type_str,
    rpc::error::SnarkosRequestError,
    state::{AgentId, CannonId, EnvId, NodeKey, TimelineId},
};
use strum_macros::AsRefStr;
use thiserror::Error;
use tokio::task::JoinError;

use crate::{
    apply::error::{SchemaError, StorageError},
    cannon::error::{AuthorizeError, CannonError},
};

#[derive(Debug, Error, AsRefStr)]
pub enum EnvRequestError {
    #[error("environment {0} not found")]
    MissingEnv(EnvId),
    #[error(transparent)]
    AgentRequestError(SnarkosRequestError),
    #[error("no nodes matched the target")]
    NoMatchingNodes,
    #[error("no responsive nodes found")]
    NoResponsiveNodes,
}

impl_into_status_code!(EnvRequestError, |value| match value {
    AgentRequestError(_e) => StatusCode::INTERNAL_SERVER_ERROR,
    _ => StatusCode::NOT_FOUND,
});

impl_into_type_str!(EnvRequestError, |value| match value {
    AgentRequestError(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    _ => value.as_ref().to_string(),
});

impl Serialize for EnvRequestError {
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
pub enum ExecutionError {
    #[error(transparent)]
    AotCmdError(#[from] AotCmdError),
    #[error("an agent is offline, so the test cannot complete")]
    AgentOffline,
    #[error("env `{0}` not found")]
    EnvNotFound(EnvId),
    #[error(transparent)]
    Cannon(#[from] CannonError),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error("env `{0}` timeline `{1}` not found")]
    TimelineNotFound(EnvId, TimelineId),
    #[error("env timeline is already being executed")]
    TimelineAlreadyStarted,
    #[error("unknown cannon: `{0}`")]
    UnknownCannon(CannonId),
    #[error(transparent)]
    AuthorizeError(#[from] AuthorizeError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl_into_status_code!(ExecutionError, |value| match value {
    Cannon(e) => e.into(),
    Storage(e) => e.into(),
    _ => StatusCode::INTERNAL_SERVER_ERROR,
});

impl_into_type_str!(ExecutionError, |value| match value {
    AuthorizeError(e) => format!("{}.{}", value.as_ref(), &String::from(e)),
    AotCmdError(e) => format!("{}.{}", value.as_ref(), &String::from(e)),
    Cannon(e) => format!("{}.{}", value.as_ref(), &String::from(e)),
    Storage(e) => format!("{}.{}", value.as_ref(), &String::from(e)),
    _ => value.as_ref().to_string(),
});

impl Serialize for ExecutionError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", &String::from(self))?;

        match self {
            Self::AotCmdError(e) => state.serialize_field("error", &e.to_string()),
            Self::Cannon(e) => state.serialize_field("error", &e.to_string()),
            _ => state.serialize_field("error", &self.to_string()),
        }?;

        state.end()
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
    #[error("insufficient number of agents to satisfy the request: have {0}: need {1}")]
    InsufficientAgentCount(usize, usize),
    #[error("could not find any agents for node {0}")]
    NoAvailableAgents(NodeKey),
}

impl_into_status_code!(DelegationError, |value| match value {
    AgentAlreadyClaimed(_, _) => StatusCode::IM_USED,
    AgentNotFound(_, _) => StatusCode::NOT_FOUND,
    AgentMissingMode(_, _) => StatusCode::BAD_REQUEST,
    InsufficientAgentCount(_, _) | NoAvailableAgents(_) => {
        StatusCode::SERVICE_UNAVAILABLE
    }
});

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
    #[error(transparent)]
    Reconcile(#[from] ReconcileError),
    #[error(transparent)]
    Cannon(#[from] CannonError),
}

impl_into_status_code!(PrepareError, |value| match value {
    DuplicateNodeKey(_) | MultipleStorage | NodeHas0Replicas => StatusCode::BAD_REQUEST,
    MissingStorage => StatusCode::NOT_FOUND,
    Cannon(e) => e.into(),
    Reconcile(e) => e.into(),
});

impl_into_type_str!(PrepareError, |value| match value {
    Reconcile(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    _ => value.as_ref().to_string(),
});

impl Serialize for PrepareError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", &String::from(self))?;

        match self {
            Self::Reconcile(e) => state.serialize_field("error", &e.to_string()),
            _ => state.serialize_field("error", &self.to_string()),
        }?;

        state.end()
    }
}

#[derive(Debug, Error, AsRefStr)]
pub enum CleanupError {
    #[error("env `{0}` not found")]
    EnvNotFound(EnvId),
    #[error("env `{0}` timeline `{1}` not found")]
    TimelineNotFound(EnvId, TimelineId),
}

impl_into_status_code!(CleanupError, |_| StatusCode::NOT_FOUND);

#[derive(Debug, Error, AsRefStr)]
pub enum ReconcileError {
    #[error("env `{0}` not found")]
    EnvNotFound(EnvId),
    #[error("expected internal agent peer for node with key {key}")]
    ExpectedInternalAgentPeer { key: NodeKey },
}

impl_into_status_code!(ReconcileError, |value| match value {
    EnvNotFound(_) | ExpectedInternalAgentPeer { .. } => StatusCode::NOT_FOUND,
});

#[derive(Debug, Error, AsRefStr)]
pub enum EnvError {
    #[error(transparent)]
    Cannon(#[from] CannonError),
    #[error(transparent)]
    Cleanup(#[from] CleanupError),
    #[error("delegation errors occured:{}", .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"))]
    Delegation(Vec<DelegationError>),
    #[error(transparent)]
    Execution(#[from] ExecutionError),
    #[error(transparent)]
    Prepare(#[from] PrepareError),
    #[error(transparent)]
    Reconcile(#[from] ReconcileError),
    #[error(transparent)]
    Schema(#[from] SchemaError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl_into_status_code!(EnvError, |value| match value {
    Cannon(e) => e.into(),
    Cleanup(e) => e.into(),
    Delegation(e) => e.iter().fold(StatusCode::OK, |acc, x| acc.max(x.into())),
    Execution(e) => e.into(),
    Prepare(e) => e.into(),
    Reconcile(e) => e.into(),
    Schema(e) => e.into(),
    Storage(e) => e.into(),
});

impl_into_type_str!(EnvError, |value| match value {
    Cannon(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Cleanup(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    Delegation(e) => format!(
        "{}.{}",
        value.as_ref(),
        e.iter().map(|x| x.as_ref()).collect::<Vec<_>>().join(",")
    ),
    Execution(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Prepare(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Reconcile(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    Schema(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Storage(e) => format!("{}.{}", value.as_ref(), String::from(e)),
});

impl Serialize for EnvError {
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
