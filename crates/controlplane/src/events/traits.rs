use std::sync::Arc;

use snops_common::state::{AgentId, AgentState, EnvId, InternedId, NodeKey};

use super::{AgentEvent, Event, EventFilter, EventKind, EventKindFilter, TransactionEvent};
use crate::{cannon::context::ExecutionContext, state::Agent};

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
    fn with_transaction(self, transaction: Arc<String>) -> Event;
    fn with_cannon(self, cannon: InternedId) -> Event;
    fn with_cannon_ctx(self, ctx: &ExecutionContext, transaction: Arc<String>) -> Event;
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

    fn with_transaction(self, transaction: Arc<String>) -> Event {
        let mut event = self.into();
        event.transaction = Some(transaction);
        event
    }

    fn with_cannon(self, cannon: InternedId) -> Event {
        let mut event = self.into();
        event.cannon = Some(cannon);
        event
    }

    fn with_cannon_ctx(self, ctx: &ExecutionContext, transaction: Arc<String>) -> Event {
        let mut event = self.into();
        event.cannon = Some(ctx.id);
        event.env = Some(ctx.env_id);
        event.transaction = Some(transaction);
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

impl From<TransactionEvent> for Event {
    fn from(kind: TransactionEvent) -> Self {
        Self::new(EventKind::Transaction(kind))
    }
}
