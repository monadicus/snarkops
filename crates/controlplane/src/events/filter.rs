use std::sync::Arc;

use snops_common::{
    node_targets::NodeTargets,
    state::{AgentId, EnvId, InternedId, NodeKey},
};

use super::{Event, EventKindFilter};

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
    /// Filter by transaction ID
    TransactionIs(Arc<String>),
    /// Filter by cannon ID
    CannonIs(InternedId),
    /// Filter by event kind
    EventIs(EventKindFilter),
    /// Filter by node key
    NodeKeyIs(NodeKey),
    /// Filter by node target
    NodeTargetIs(NodeTargets),
}

impl Event {
    pub fn matches(&self, filter: &EventFilter) -> bool {
        match filter {
            EventFilter::Unfiltered => true,
            EventFilter::AllOf(filters) => filters.iter().all(|f| self.matches(f)),
            EventFilter::AnyOf(filters) => filters.iter().any(|f| self.matches(f)),
            EventFilter::OneOf(filters) => filters.iter().filter(|f| self.matches(f)).count() == 1,
            EventFilter::Not(f) => !self.matches(f),
            EventFilter::AgentIs(agent) => self.agent == Some(*agent),
            EventFilter::EnvIs(env) => self.env == Some(*env),
            EventFilter::TransactionIs(transaction) => {
                self.transaction.as_ref() == Some(transaction)
            }
            EventFilter::CannonIs(cannon) => self.cannon == Some(*cannon),
            EventFilter::EventIs(kind) => self.kind.filter() == *kind,
            EventFilter::NodeKeyIs(node_key) => self.node_key.as_ref() == Some(node_key),
            EventFilter::NodeTargetIs(node_targets) => self
                .node_key
                .as_ref()
                .is_some_and(|key| node_targets.matches(key)),
        }
    }
}
