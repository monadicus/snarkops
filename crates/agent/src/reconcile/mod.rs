use std::{collections::HashSet, time::Duration};

use indexmap::IndexSet;

mod agent;
mod checkpoint;
mod files;
pub use files::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReconcileCondition {
    /// A file is being downloaded.
    PendingDownload(String),
    /// A file is being unpacked.
    PendingUnpack(String),
    /// A process is being spawned / confirmed
    PendingProcess(String),
}

trait Reconcile<T, E> {
    async fn reconcile(&self) -> Result<ReconcileStatus<T>, E>;
}

pub struct ReconcileStatus<T> {
    pub inner: Option<T>,
    pub requeue_after: Option<Duration>,
    pub conditions: IndexSet<ReconcileCondition>,
}

impl<T: Default> Default for ReconcileStatus<T> {
    fn default() -> Self {
        Self::new(Some(Default::default()))
    }
}

impl<T> ReconcileStatus<T> {
    pub fn new(inner: Option<T>) -> Self {
        Self {
            inner,
            requeue_after: None,
            conditions: IndexSet::new(),
        }
    }

    pub fn empty() -> Self {
        Self::new(None)
    }

    pub fn is_requeue(&self) -> bool {
        self.requeue_after.is_some()
    }

    pub fn replace<U>(&self, inner: Option<U>) -> ReconcileStatus<U> {
        ReconcileStatus {
            inner,
            requeue_after: self.requeue_after,
            conditions: self.conditions.clone(),
        }
    }

    pub fn emptied<U>(&self) -> ReconcileStatus<U> {
        ReconcileStatus {
            inner: None,
            requeue_after: self.requeue_after,
            conditions: self.conditions.clone(),
        }
    }

    pub fn take(self) -> Option<T> {
        self.inner
    }

    pub fn take_conditions(&mut self) -> IndexSet<ReconcileCondition> {
        std::mem::take(&mut self.conditions)
    }

    pub fn requeue_after(mut self, duration: Duration) -> Self {
        self.requeue_after = Some(duration);
        self
    }

    pub fn add_condition(mut self, condition: ReconcileCondition) -> Self {
        self.conditions.insert(condition);
        self
    }

    pub fn add_conditions(mut self, conditions: HashSet<ReconcileCondition>) -> Self {
        self.conditions.extend(conditions);
        self
    }
}
