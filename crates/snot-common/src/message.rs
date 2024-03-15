#[cfg(any(feature = "server", feature = "client"))]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "server", derive(Serialize))]
#[cfg_attr(feature = "client", derive(Deserialize))]
pub enum ServerMessage {
    Ping,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "server", derive(Deserialize))]
#[cfg_attr(feature = "client", derive(Serialize))]
pub enum ClientMessage {
    Pong,
}
