use std::{
    fmt::{Display, Write},
    net::SocketAddr,
    str::FromStr,
    sync::atomic::{AtomicUsize, Ordering},
};

use clap::Parser;
use lasso::Spur;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{de::Error, Deserialize, Serialize};

use crate::INTERN;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentId(Spur);

pub type StorageId = usize;
pub type EnvId = usize;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub enum AgentState {
    #[default]
    // A node in the inventory can function as a transaction cannon
    Inventory,
    /// Test id mapping to node state
    Node(EnvId, NodeState),
}

impl AgentState {
    pub fn map_node<F>(self, f: F) -> AgentState
    where
        F: Fn(NodeState) -> NodeState,
    {
        match self {
            Self::Inventory => Self::Inventory,
            Self::Node(id, state) => Self::Node(id, f(state)),
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct PortConfig {
    /// Specify the IP address and port for the node server
    #[clap(long = "node", default_value_t = 4130)]
    pub node: u16,

    /// Specify the IP address and port for the BFT
    #[clap(long = "bft", default_value_t = 5000)]
    pub bft: u16,

    /// Specify the IP address and port for the REST server
    #[clap(long = "rest", default_value_t = 3030)]
    pub rest: u16,

    /// Specify the port for the metrics
    #[clap(long = "metrics", default_value_t = 9000)]
    pub metrics: u16,
}

impl Display for PortConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bft: {}, node: {}, rest: {}",
            self.bft, self.node, self.rest
        )
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Parser)]
pub struct AgentMode {
    /// Enable running a validator node
    #[arg(long)]
    pub validator: bool,

    /// Enable running a prover node
    #[arg(long)]
    pub prover: bool,

    /// Enable running a client node
    #[arg(long)]
    pub client: bool,

    /// Enable functioning as a compute target when inventoried
    #[arg(long)]
    pub compute: bool,
}

impl From<AgentMode> for u8 {
    fn from(mode: AgentMode) -> u8 {
        (mode.validator as u8)
            | (mode.prover as u8) << 1
            | (mode.client as u8) << 2
            | (mode.compute as u8) << 3
    }
}

impl From<u8> for AgentMode {
    fn from(mode: u8) -> Self {
        Self {
            validator: mode & 1 != 0,
            prover: mode & 1 << 1 != 0,
            client: mode & 1 << 2 != 0,
            compute: mode & 1 << 3 != 0,
        }
    }
}

impl Display for AgentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        if self.validator {
            s.push_str("validator");
        }
        if self.prover {
            if !s.is_empty() {
                s.push_str(", ");
            }
            s.push_str("prover");
        }
        if self.client {
            if !s.is_empty() {
                s.push_str(", ");
            }
            s.push_str("client");
        }
        if self.compute {
            if !s.is_empty() {
                s.push_str(", ");
            }
            s.push_str("compute");
        }

        f.write_str(&s)
    }
}

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

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct NodeKey {
    pub ty: NodeType,
    pub id: String,
    /// The node key namespace. If `None`, is a local node.
    pub ns: Option<String>, // TODO: string interning or otherwise not duplicating namespace
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    static ref AGENT_ID_REGEX: Regex = Regex::new(r"^[A-Za-z0-9][A-Za-z0-9\-_.]{0,63}$").unwrap();
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

impl Serialize for NodeKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(INTERN.get_or_intern(format!("agent-{}", id)))
    }
}

impl FromStr for AgentId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !AGENT_ID_REGEX.is_match(s) {
            return Err("invalid agent id: expected pattern [A-Za-z0-9][A-Za-z0-9\\-_.]{{,63}}");
        }

        Ok(AgentId(INTERN.get_or_intern(s)))
    }
}

impl<'de> Deserialize<'de> for AgentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <&str>::deserialize(deserializer)?;
        Self::from_str(s).map_err(D::Error::custom)
    }
}

impl Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", INTERN.resolve(&self.0))
    }
}

impl Serialize for AgentId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
