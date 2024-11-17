use std::{collections::HashSet, fmt::Display, time::Duration};

use indexmap::IndexSet;

pub mod agent;
mod checkpoint;
pub mod command;
mod files;
pub use files::*;
use snops_common::state::TransferId;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReconcileCondition {
    /// A file is being transferred.
    PendingTransfer(String, TransferId),
    /// A file is being unpacked.
    PendingUnpack(String),
    /// A process is being spawned / confirmed. Could be starting the node or
    /// manipulating the ledger
    PendingProcess(String),
    /// A tranfer was started and interrupted.
    InterruptedTransfer(String, TransferId, String),
    /// A file is missing and cannot be downloaded at the moment.
    MissingFile(String),
    /// Waiting to reconnect to the controlplane
    PendingConnection,
}

pub trait Reconcile<T, E> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<T>, E>;
}

pub struct ReconcileStatus<T> {
    pub scopes: Vec<String>,
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
            scopes: Vec::new(),
            inner,
            requeue_after: None,
            conditions: IndexSet::new(),
        }
    }

    pub fn with(inner: T) -> Self {
        Self::new(Some(inner))
    }

    pub fn empty() -> Self {
        Self::new(None)
    }

    pub fn is_requeue(&self) -> bool {
        self.requeue_after.is_some()
    }

    pub fn replace<U>(&self, inner: Option<U>) -> ReconcileStatus<U> {
        ReconcileStatus {
            scopes: self.scopes.clone(),
            inner,
            requeue_after: self.requeue_after,
            conditions: self.conditions.clone(),
        }
    }

    pub fn emptied<U>(&self) -> ReconcileStatus<U> {
        ReconcileStatus {
            inner: None,
            scopes: self.scopes.clone(),
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

    pub fn add_scope(mut self, scope: impl Display) -> Self {
        self.scopes.push(scope.to_string());
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
