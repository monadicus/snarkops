use std::{fmt::Display, time::Duration};

use indexmap::IndexSet;
use serde::{Deserialize, Serialize};

use super::TransferId;

#[derive(Debug, Copy, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReconcileOptions {
    /// When true, the reconciler will fetch the latest env info
    pub refetch_info: bool,
    /// When true, the reconciler will force the node to shut down
    pub force_shutdown: bool,
    /// When true, the reconciler will clear the last height
    pub clear_last_height: bool,
}

impl ReconcileOptions {
    pub fn union(self, other: Self) -> Self {
        Self {
            refetch_info: self.refetch_info || other.refetch_info,
            force_shutdown: self.force_shutdown || other.force_shutdown,
            clear_last_height: self.clear_last_height || other.clear_last_height,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "condition", rename_all = "snake_case")]
pub enum ReconcileCondition {
    /// A file is being transferred.
    PendingTransfer { source: String, id: TransferId },
    /// A process is being spawned / confirmed. Could be starting the node or
    /// manipulating the ledger
    PendingProcess { process: String },
    /// A tranfer was started and interrupted.
    InterruptedTransfer {
        source: String,
        id: TransferId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// A modify operation was started and interrupted.
    InterruptedModify { reason: String },
    /// A file is missing and cannot be downloaded at the moment.
    MissingFile { path: String },
    /// Waiting to reconnect to the controlplane
    PendingConnection,
    /// Waiting for the node to be shut down
    PendingShutdown,
    /// Waiting for the node to start up
    PendingStartup,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReconcileStatus<T> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner: Option<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requeue_after: Option<Duration>,
    #[serde(default, skip_serializing_if = "IndexSet::is_empty")]
    pub conditions: IndexSet<ReconcileCondition>,
}

impl<T: Eq> Eq for ReconcileStatus<T> {}
impl<T: PartialEq> PartialEq for ReconcileStatus<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
            && self.conditions == other.conditions
            && self.scopes == other.scopes
            && self.requeue_after == other.requeue_after
    }
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

    pub fn emptied<U>(&self) -> ReconcileStatus<U> {
        ReconcileStatus {
            inner: None,
            scopes: self.scopes.clone(),
            requeue_after: self.requeue_after,
            conditions: self.conditions.clone(),
        }
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
}
