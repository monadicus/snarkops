use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use snops_common::{
    node_targets::NodeTargets,
    rpc::error::ReconcileError,
    state::{AgentId, AgentState, EnvId, LatestBlockInfo, NodeKey, NodeStatus, ReconcileStatus},
};

use crate::state::Agent;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub created_at: DateTime<Utc>,
    pub agent: Option<AgentId>,
    pub node_key: Option<NodeKey>,
    pub env: Option<EnvId>,
    pub kind: EventKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EventKind {
    /// An agent connects to the control plane
    AgentConnected,
    /// An agent completes a handshake with the control plane
    AgentHandshakeComplete,
    /// An agent disconnects from the control plane
    AgentDisconnected,
    /// An agent finishes a reconcile
    ReconcileComplete,
    /// An agent updates its reconcile status
    Reconcile(ReconcileStatus<()>),
    /// An error occurs during reconcile
    ReconcileError(ReconcileError),
    /// An agent emits a node status
    NodeStatus(NodeStatus),
    /// An agent emits a block update
    Block(LatestBlockInfo),
}

#[derive(Clone, Copy, Debug)]
pub enum EventKindFilter {
    AgentConnected,
    AgentHandshakeComplete,
    AgentDisconnected,
    ReconcileComplete,
    Reconcile,
    ReconcileError,
    NodeStatus,
    Block,
}

pub enum EventFilter {
    /// No filter
    Unfiltered,

    /// Logical AND of filters
    AllOf(Vec<EventFilter>),
    /// Logical OR of filters
    AnyOf(Vec<EventFilter>),
    /// Logical XOR of filters
    OneOf(Vec<EventFilter>),
    /// Logical NOT of filter
    Not(Box<EventFilter>),

    /// Filter by agent ID
    AgentIs(AgentId),
    /// Filter by environment ID
    EnvIs(EnvId),
    /// Filter by event kind
    EventIs(EventKindFilter),
    /// Filter by node key
    NodeKeyIs(NodeKey),
    /// Filter by node target
    NodeTargetIs(NodeTargets),
}

impl Event {
    pub fn new(kind: EventKind) -> Self {
        Self {
            created_at: Utc::now(),
            agent: None,
            node_key: None,
            env: None,
            kind,
        }
    }

    pub fn replace_kind(&self, kind: EventKind) -> Self {
        Self {
            created_at: Utc::now(),
            agent: self.agent,
            node_key: self.node_key.clone(),
            env: self.env,
            kind,
        }
    }

    pub fn with_agent(mut self, agent: &Agent) -> Self {
        self.agent = Some(agent.id);
        if let AgentState::Node(env_id, node) = &agent.state {
            self.node_key = Some(node.node_key.clone());
            self.env = Some(*env_id);
        }
        self
    }

    pub fn with_env(mut self, env_id: EnvId) -> Self {
        self.env = Some(env_id);
        self
    }
}
