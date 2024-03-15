use std::str::FromStr;

use lazy_static::lazy_static;
use regex::Regex;
use serde::{de::Error, Deserialize, Serialize};

/// Desired state for an agent.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DesiredState {
    pub online: bool,
    pub ty: Option<NodeType>,
}

impl DesiredState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_online(mut self, online: bool) -> Self {
        self.online = online;
        self
    }

    pub fn with_type(mut self, ty: Option<NodeType>) -> Self {
        self.ty = ty;
        self
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
pub enum NodeType {
    Client,
    Validator,
    Prover,
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
        let ty = match &captures["ty"] {
            "client" => NodeType::Client,
            "validator" => NodeType::Validator,
            "prover" => NodeType::Prover,
            _ => unreachable!(),
        };

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
