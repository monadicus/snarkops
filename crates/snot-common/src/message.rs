#[cfg(any(feature = "server", feature = "client"))]
use serde::{Deserialize, Serialize};

use crate::state::NodeState;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "server", derive(Serialize))]
#[cfg_attr(feature = "client", derive(Deserialize))]
pub enum ServerMessage {
    StateReconcile(NodeState),
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "server", derive(Deserialize))]
#[cfg_attr(feature = "client", derive(Serialize))]
pub enum ClientMessage {
    StateReconciled,
    StateReconcileFail(String),
    // RequestPeerAddress,
}
