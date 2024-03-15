#[cfg(any(feature = "server", feature = "client"))]
use serde::{Deserialize, Serialize};

use crate::state::DesiredState;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "server", derive(Serialize))]
#[cfg_attr(feature = "client", derive(Deserialize))]
pub enum ServerMessage {
    SetState(DesiredState),
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "server", derive(Deserialize))]
#[cfg_attr(feature = "client", derive(Serialize))]
pub enum ClientMessage {}
