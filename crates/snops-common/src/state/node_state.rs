use std::{collections::HashMap, net::SocketAddr};

use super::{AgentId, HeightRequest, NodeKey, NodeType};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeState {
    pub node_key: NodeKey,
    pub ty: NodeType,
    pub private_key: KeyState,
    /// Increment the usize whenever the request is updated.
    pub height: (usize, HeightRequest),

    pub online: bool,
    pub peers: Vec<AgentPeer>,
    pub validators: Vec<AgentPeer>,
    pub env: HashMap<String, String>,
}

/// A representation of which key to use for the agent.
#[derive(Default, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum KeyState {
    /// No private key provided
    #[default]
    None,
    /// A private key is provided by the agent
    Local,
    /// A literal private key
    Literal(String),
    // TODO: generated?/new
}

impl From<Option<String>> for KeyState {
    fn from(s: Option<String>) -> Self {
        match s {
            Some(s) => Self::Literal(s),
            None => Self::None,
        }
    }
}

impl KeyState {
    pub fn try_string(&self) -> Option<String> {
        match self {
            Self::Literal(s) => Some(s.to_owned()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub enum AgentPeer {
    Internal(AgentId, u16),
    External(SocketAddr),
}

impl AgentPeer {
    /// Get the port from the peer
    pub fn port(&self) -> u16 {
        match self {
            Self::Internal(_, port) => *port,
            Self::External(addr) => addr.port(),
        }
    }

    /// Return a new peer with the given port.
    pub fn with_port(&self, port: u16) -> Self {
        match self {
            Self::Internal(ip, _) => Self::Internal(*ip, port),
            Self::External(addr) => Self::External(SocketAddr::new(addr.ip(), port)),
        }
    }
}
