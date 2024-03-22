use std::{
    fmt::{Display, Write},
    net::SocketAddr,
    str::FromStr,
};

use lazy_static::lazy_static;
use regex::Regex;
use serde::{de::Error, Deserialize, Serialize};

type AgentId = usize;
type StorageId = usize;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub enum AgentState {
    #[default]
    // A node in the inventory can function as a transaction cannon
    Inventory,
    Node(StorageId, NodeState),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState {
    pub ty: NodeType,
    pub private_key: Option<String>,
    pub height: (usize, HeightRequest),

    pub online: bool,
    pub peers: Vec<AgentPeer>,
    pub validators: Vec<AgentPeer>,
}

// // agent code
// impl AgentState {
//     async fn reconcile(&self, target: AgentState) {
//         // assert that we can actually move from self to target
//         // if not, return ReconcileFailed

//         if self.peers != target.peers {
//             if self.online {
//                 self.turn_offline();
//             }

//             // make change to peers
//             self.peers = target.peers;
//             // make the change in snarkos

//             // restore online state
//         }

//         // and do the rest of these fields

//         // return StateReconciled(self)
//     }
// }

// #[derive(Debug, Default, Clone, Serialize, Deserialize)]
// pub enum AgentState {
//     Inventory,
//     Node(ContextRequest, ConfigRequest),
//     Cannon(/* config */),
// }

// /// Desired state for an agent's node.
// #[derive(Debug, Default, Clone, Serialize, Deserialize)]
// pub struct ContextRequest {
//     pub id: usize,
//     pub ty: NodeType,
//     pub storage: StorageId,
//     pub starting_height: Option<u32>,
// }

// #[derive(Debug, Default, Clone, Serialize, Deserialize)]
// pub struct ConfigRequest {
//     pub id: usize,
//     pub online: bool,
//     pub peers: Vec<AgentPeer>,
//     pub validators: Vec<AgentPeer>,
//     pub next_height: Option<HeightRequest>,
// }

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub enum HeightRequest {
    #[default]
    Top,
    Absolute(u32),
    // the control plane doesn't know the heights the nodes are at
    // TruncateHeight(u32),
    // TruncateTime(i64),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AgentPeer {
    Internal(AgentId),
    External(SocketAddr),
}

// /// The state reported by an agent.
// #[derive(Debug, Default, Clone, Serialize, Deserialize, Hash)]
// pub struct ResolvedState {
//     /// The timestamp of the last update.
//     pub timestamp: i64, // TODO: chrono

//     // pub online: bool,
//     // pub config_ty: Option<NodeType>,

//     pub current_state: State,

//     pub genesis_hash: Option<String>,
//     pub config_peers: Option<Vec<SocketAddr>>,
//     pub config_validators: Option<Vec<SocketAddr>>,
//     pub snarkos_peers: Option<Vec<SocketAddr>>,
//     pub snarkos_validators: Option<Vec<SocketAddr>>,
//     pub block_height: Option<u32>,
//     pub block_timestamp: Option<i64>,
// }

// impl ConfigRequest {
//     pub fn new() -> Self {
//         Self::default()
//     }

//     pub fn with_online(mut self, online: bool) -> Self {
//         self.online = online;
//         self
//     }

//     pub fn with_type(mut self, ty: Option<NodeType>) -> Self {
//         self.ty = ty;
//         self
//     }
// }

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct NodeKey {
    pub ty: NodeType,
    pub id: String,
    /// The node key namespace. If `None`, is a local node.
    pub ns: Option<String>, // TODO: string interning or otherwise not duplicating namespace
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    Client,
    Validator,
    Prover,
}

impl NodeType {
    pub fn flag(self) -> &'static str {
        match self {
            Self::Client => "--client",
            Self::Validator => "--validator",
            Self::Prover => "--prover",
        }
    }
}

impl Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Client => f.write_str("client"),
            Self::Validator => f.write_str("validator"),
            Self::Prover => f.write_str("prover"),
        }
    }
}

impl FromStr for NodeType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "client" => Ok(Self::Client),
            "validator" => Ok(Self::Validator),
            "prover" => Ok(Self::Prover),
            _ => Err("invalid node type string"),
        }
    }
}

lazy_static! {
    static ref NODE_KEY_REGEX: Regex = Regex::new(
        r"^(?P<ty>client|validator|prover)\/(?P<id>[A-Za-z0-9\-]+)(?:@(?P<ns>[A-Za-z0-9\-]+))?$"
    )
    .unwrap();
}

impl FromStr for NodeKey {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some(captures) = NODE_KEY_REGEX.captures(s) else {
            return Err("invalid node key string");
        };

        // match the type
        let ty = NodeType::from_str(&captures["ty"]).unwrap();

        // match the node ID
        let id = String::from(&captures["id"]);

        // match the namespace
        let ns = match captures.name("ns") {
            // local; either explicitly stated, or empty
            Some(id) if id.as_str() == "local" => None,
            None => None,

            // literal namespace
            Some(id) => Some(id.as_str().into()),
        };

        Ok(Self { ty, id, ns })
    }
}

impl<'de> Deserialize<'de> for NodeKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <&str>::deserialize(deserializer)?;
        Self::from_str(s).map_err(D::Error::custom)
    }
}

impl Display for NodeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.ty, self.id)?;
        if let Some(ns) = &self.ns {
            f.write_char('@')?;
            f.write_str(ns)?;
        }

        Ok(())
    }
}
