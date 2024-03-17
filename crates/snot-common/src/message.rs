#[cfg(any(feature = "server", feature = "client"))]
use serde::{Deserialize, Serialize};

use crate::state::AgentState;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "server", derive(Serialize))]
#[cfg_attr(feature = "client", derive(Deserialize))]
pub enum ServerMessage {
    Reconcile(usize, AgentState),
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "server", derive(Deserialize))]
#[cfg_attr(feature = "client", derive(Serialize))]
pub enum ClientMessage {
    /// A successful reconcile of the desired state
    ReconcileSuccess(usize),
    /// A realtime status update of the reconcile process
    ReconcileStatus(usize, String),
    /// A reconcile was skipped because the state was already reconciled a newer
    /// state was pushed
    ReconcileSkipped(usize),
    /// The latest reconcile failed
    ReconcileFail(usize, String),
    // RequestPeerAddress,
}
