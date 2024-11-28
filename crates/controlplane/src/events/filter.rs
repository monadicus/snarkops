use super::{Event, EventFilter, EventKind, EventKindFilter};

impl EventKind {
    pub fn filter(&self, filter: &EventKindFilter) -> bool {
        matches!(
            (self, filter),
            (EventKind::AgentConnected, EventKindFilter::AgentConnected)
                | (
                    EventKind::AgentHandshakeComplete,
                    EventKindFilter::AgentHandshakeComplete
                )
                | (
                    EventKind::AgentDisconnected,
                    EventKindFilter::AgentDisconnected
                )
                | (
                    EventKind::ReconcileComplete,
                    EventKindFilter::ReconcileComplete
                )
                | (EventKind::Reconcile(_), EventKindFilter::Reconcile)
                | (
                    EventKind::ReconcileError(_),
                    EventKindFilter::ReconcileError
                )
                | (EventKind::NodeStatus(_), EventKindFilter::NodeStatus)
                | (EventKind::Block(_), EventKindFilter::Block)
        )
    }
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
            EventFilter::EventIs(kind) => self.kind.filter(kind),
            EventFilter::NodeKeyIs(node_key) => self.node_key.as_ref() == Some(node_key),
            EventFilter::NodeTargetIs(node_targets) => self
                .node_key
                .as_ref()
                .is_some_and(|key| node_targets.matches(key)),
        }
    }
}
