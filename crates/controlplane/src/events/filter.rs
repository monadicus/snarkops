use super::{Event, EventFilter};

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
