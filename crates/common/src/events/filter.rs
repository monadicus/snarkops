use std::{fmt::Display, sync::Arc};

use super::{Event, EventKindFilter};
use crate::{
    node_targets::NodeTargets,
    state::{AgentId, EnvId, InternedId, NodeKey},
};

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
            EventFilter::EventIs(kind) => self.content.filter() == *kind,
            EventFilter::NodeKeyIs(node_key) => self.node_key.as_ref() == Some(node_key),
            EventFilter::NodeTargetIs(node_targets) => self
                .node_key
                .as_ref()
                .is_some_and(|key| node_targets.matches(key)),
        }
    }
}

fn event_filter_vec(filters: &[EventFilter]) -> String {
    filters
        .iter()
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

impl Display for EventFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventFilter::Unfiltered => write!(f, "unfiltered"),
            EventFilter::AllOf(vec) => write!(f, "all-of({})", event_filter_vec(vec)),
            EventFilter::AnyOf(vec) => write!(f, "any-of({})", event_filter_vec(vec)),
            EventFilter::OneOf(vec) => write!(f, "one-of({})", event_filter_vec(vec)),
            EventFilter::Not(event_filter) => write!(f, "not({})", event_filter),
            EventFilter::AgentIs(id) => write!(f, "agent-is({id})"),
            EventFilter::EnvIs(id) => write!(f, "env-is({id})"),
            EventFilter::TransactionIs(str) => write!(f, "transaction-is({str})"),
            EventFilter::CannonIs(id) => write!(f, "cannon-is({id})"),
            EventFilter::EventIs(event) => write!(f, "event-is({event})"),
            EventFilter::NodeKeyIs(node_key) => write!(f, "node-key-is({node_key})"),
            EventFilter::NodeTargetIs(node_targets) => write!(f, "node-target-is({node_targets})"),
        }
    }
}
