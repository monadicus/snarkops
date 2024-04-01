use std::{collections::HashSet, fmt::Display, net::SocketAddr, str::FromStr};

use fixedbitset::FixedBitSet;
use indexmap::IndexMap;
use lazy_static::lazy_static;
use serde::{de::Visitor, Deserialize, Deserializer, Serialize};
use snot_common::{
    lasso::Spur,
    set::{MaskBit, MASK_PREFIX_LEN},
    state::{AgentId, HeightRequest, KeyState, NodeState, NodeType},
    INTERN,
};

use super::{NodeKey, NodeTargets};

/// A document describing the node infrastructure for a test.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub name: String,
    pub description: Option<String>,

    #[serde(default)]
    pub external: IndexMap<NodeKey, ExternalNode>,

    #[serde(default)]
    pub nodes: IndexMap<NodeKey, Node>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ExternalNode {
    // NOTE: these fields must be validated at runtime, because validators require `bft` to be set,
    // and non-validators require `node` to be set
    // rest is required to be a target of the tx-cannon
    pub bft: Option<SocketAddr>,
    pub node: Option<SocketAddr>,
    pub rest: Option<SocketAddr>,
}

// zander forgive me -isaac
fn please_be_online() -> bool {
    true
}

/// Parse the labels as strings, but intern them on load
fn get_label<'de, D>(deserializer: D) -> Result<HashSet<Spur>, D::Error>
where
    D: Deserializer<'de>,
{
    let labels = Vec::<String>::deserialize(deserializer)?;
    Ok(labels
        .into_iter()
        .map(|label| INTERN.get_or_intern(label))
        .collect())
}

// TODO: could use some more clarification on some of these fields
/// A node in the testing infrastructure.
#[derive(Deserialize, Debug, Clone)]
pub struct Node {
    #[serde(default = "please_be_online")]
    pub online: bool,
    /// When specified, creates a group of nodes, all with the same
    /// configuration.
    pub replicas: Option<usize>,
    /// The private key to start the node with.
    pub key: Option<KeySource>,
    /// Height of ledger to inherit.
    ///
    /// * When null, a ledger is created when the node is started.
    /// * When zero, the ledger is empty and only the genesis block is
    ///   inherited.
    pub height: Option<usize>,

    /// When specified, agents must have these labels
    #[serde(default, deserialize_with = "get_label")]
    pub labels: HashSet<Spur>,

    /// When specified, an agent must have this id. Overrides the labels field.
    #[serde(default)]
    pub agent: Option<AgentId>,

    /// List of validators for the node to connect to
    #[serde(default)]
    pub validators: NodeTargets,

    /// List of peers for the node to connect to
    #[serde(default)]
    pub peers: NodeTargets,
}

impl Node {
    pub fn into_state(&self, ty: NodeType) -> NodeState {
        NodeState {
            ty,
            private_key: KeyState::None,

            // TODO
            height: (0, HeightRequest::Top),

            online: self.online,

            // these are resolved later
            validators: vec![],
            peers: vec![],
        }
    }

    pub fn mask(&self, key: &NodeKey, labels: &[Spur]) -> FixedBitSet {
        let mut mask = FixedBitSet::with_capacity(labels.len() + MASK_PREFIX_LEN);

        // validator/prover/client
        mask.insert(key.ty.bit());

        // local private key
        if matches!(self.key, Some(KeySource::Local)) {
            mask.insert(MaskBit::LocalPrivateKey as usize);
        }

        // labels
        for (i, label) in labels.iter().enumerate() {
            if self.labels.contains(label) {
                mask.insert(i + MASK_PREFIX_LEN);
            }
        }
        mask
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum KeySource {
    /// Private key owned by the agent
    Local,
    /// APrivateKey1zkp...
    Literal(String),
    /// committee.0 or committee.$ (for replicas)
    Committee(Option<usize>),
    /// accounts.0 or accounts.$ (for replicas)
    Named(String, Option<usize>),
}

struct KeySourceVisitor;

impl<'de> Visitor<'de> for KeySourceVisitor {
    type Value = KeySource;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string that represents an aleo private key, or a file from storage")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        KeySource::from_str(v).map_err(E::custom)
    }
}

impl<'de> Deserialize<'de> for KeySource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(KeySourceVisitor)
    }
}

impl Serialize for KeySource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl FromStr for KeySource {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // use KeySource::Literal(String) when the string is 59 characters long and starts with "APrivateKey1zkp"
        // use KeySource::Commitee(Option<usize>) when the string is "committee.0" or "committee.$"
        // use KeySource::Named(String, Option<usize>) when the string is "\w+.0" or "\w+.$"

        if s == "local" {
            return Ok(KeySource::Local);
        }
        // aleo private key
        else if s.len() == 59 && s.starts_with("APrivateKey1") {
            return Ok(KeySource::Literal(s.to_string()));

        // committee key
        } else if let Some(index) = s.strip_prefix("committee.") {
            if index == "$" {
                return Ok(KeySource::Committee(None));
            }
            let replica = index
                .parse()
                .map_err(|_e| "committee index must be a positive number")?;
            return Ok(KeySource::Committee(Some(replica)));
        }

        // named key (using regex with capture groups)
        lazy_static! {
            static ref NAMED_KEYSOURCE_REGEX: regex::Regex =
                regex::Regex::new(r"^(?P<name>\w+)\.(?P<idx>\d+|\$)$").unwrap();
        }
        let groups = NAMED_KEYSOURCE_REGEX
            .captures(s)
            .ok_or("invalid key source")?;
        let name = groups.name("name").unwrap().as_str().to_string();
        let idx = match groups.name("idx").unwrap().as_str() {
            "$" => None,
            idx => Some(
                idx.parse()
                    .map_err(|_e| "index must be a positive number")?,
            ),
        };
        Ok(KeySource::Named(name, idx))
    }
}

impl Display for KeySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                KeySource::Local => "local".to_owned(),
                KeySource::Literal(key) => key.to_owned(),
                KeySource::Committee(None) => "committee.$".to_owned(),
                KeySource::Committee(Some(idx)) => {
                    format!("committee.{}", idx)
                }
                KeySource::Named(name, None) => format!("{}.{}", name, "$"),
                KeySource::Named(name, Some(idx)) => {
                    format!("{}.{}", name, idx)
                }
            }
        )
    }
}

impl KeySource {
    pub fn with_index(&self, idx: usize) -> Self {
        match self {
            KeySource::Committee(_) => KeySource::Committee(Some(idx)),
            KeySource::Named(name, _) => KeySource::Named(name.clone(), Some(idx)),
            _ => self.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_source_deserialization() {
        assert_eq!(
            serde_yaml::from_str::<KeySource>("committee.0").expect("foo"),
            KeySource::Committee(Some(0))
        );
        assert_eq!(
            serde_yaml::from_str::<KeySource>("committee.100").expect("foo"),
            KeySource::Committee(Some(100))
        );
        assert_eq!(
            serde_yaml::from_str::<KeySource>("committee.$").expect("foo"),
            KeySource::Committee(None)
        );

        assert_eq!(
            serde_yaml::from_str::<KeySource>("accounts.0").expect("foo"),
            KeySource::Named("accounts".to_string(), Some(0))
        );
        assert_eq!(
            serde_yaml::from_str::<KeySource>("accounts.$").expect("foo"),
            KeySource::Named("accounts".to_string(), None)
        );

        assert_eq!(
            serde_yaml::from_str::<KeySource>(
                "APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH"
            )
            .expect("foo"),
            KeySource::Literal(
                "APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH".to_string()
            )
        );

        assert!(serde_yaml::from_str::<KeySource>("committee.-100").is_err(),);
        assert!(serde_yaml::from_str::<KeySource>("accounts.-100").is_err(),);
        assert!(serde_yaml::from_str::<KeySource>("accounts._").is_err(),);
        assert!(serde_yaml::from_str::<KeySource>("accounts.*").is_err(),);
    }
}
