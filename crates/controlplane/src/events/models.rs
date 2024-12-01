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
#[serde(tag = "type")]
pub enum EventKind {
    Agent(AgentEvent),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum AgentEvent {
    /// An agent connects to the control plane
    Connected,
    /// An agent completes a handshake with the control plane
    HandshakeComplete,
    /// An agent disconnects from the control plane
    Disconnected,
    /// An agent finishes a reconcile
    ReconcileComplete,
    /// An agent updates its reconcile status
    Reconcile(ReconcileStatus<()>),
    /// An error occurs during reconcile
    ReconcileError(ReconcileError),
    /// An agent emits a node status
    NodeStatus(NodeStatus),
    /// An agent emits a block update
    BlockInfo(LatestBlockInfo),
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum EventKindFilter {
    AgentConnected,
    AgentHandshakeComplete,
    AgentDisconnected,
    AgentReconcileComplete,
    AgentReconcile,
    AgentReconcileError,
    AgentNodeStatus,
    AgentBlockInfo,
}

impl EventKind {
    pub fn filter(&self) -> EventKindFilter {
        use AgentEvent::*;
        use EventKind::*;

        match self {
            Agent(Connected) => EventKindFilter::AgentConnected,
            Agent(HandshakeComplete) => EventKindFilter::AgentHandshakeComplete,
            Agent(Disconnected) => EventKindFilter::AgentDisconnected,
            Agent(ReconcileComplete) => EventKindFilter::AgentReconcileComplete,
            Agent(Reconcile(_)) => EventKindFilter::AgentReconcile,
            Agent(ReconcileError(_)) => EventKindFilter::AgentReconcileError,
            Agent(NodeStatus(_)) => EventKindFilter::AgentNodeStatus,
            Agent(BlockInfo(_)) => EventKindFilter::AgentBlockInfo,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
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

    pub fn replace_kind(&self, kind: impl Into<Event>) -> Self {
        Self {
            created_at: Utc::now(),
            agent: self.agent,
            node_key: self.node_key.clone(),
            env: self.env,
            kind: kind.into().kind,
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

impl From<EventKindFilter> for EventFilter {
    fn from(kind: EventKindFilter) -> Self {
        EventFilter::EventIs(kind)
    }
}

pub trait EventHelpers {
    fn event(self) -> Event;
    fn with_agent(self, agent: &Agent) -> Event;
    fn with_agent_id(self, agent_id: AgentId) -> Event;
    fn with_node_key(self, node_key: NodeKey) -> Event;
    fn with_env_id(self, env_id: EnvId) -> Event;
}

impl<T: Into<Event>> EventHelpers for T {
    fn event(self) -> Event {
        self.into()
    }

    fn with_agent(self, agent: &Agent) -> Event {
        let mut event = self.into();
        event.agent = Some(agent.id);
        if let AgentState::Node(env_id, node) = &agent.state {
            event.node_key = Some(node.node_key.clone());
            event.env = Some(*env_id);
        }
        event
    }

    fn with_agent_id(self, agent_id: AgentId) -> Event {
        let mut event = self.into();
        event.agent = Some(agent_id);
        event
    }

    fn with_node_key(self, node_key: NodeKey) -> Event {
        let mut event = self.into();
        event.node_key = Some(node_key);
        event
    }

    fn with_env_id(self, env_id: EnvId) -> Event {
        let mut event = self.into();
        event.env = Some(env_id);
        event
    }
}

impl From<EventKind> for Event {
    fn from(kind: EventKind) -> Self {
        Self::new(kind)
    }
}

impl From<AgentEvent> for Event {
    fn from(kind: AgentEvent) -> Self {
        Self::new(EventKind::Agent(kind))
    }
}
