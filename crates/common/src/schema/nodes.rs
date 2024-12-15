use std::{
    net::{IpAddr, SocketAddr},
    num::NonZeroUsize,
};

use fixedbitset::FixedBitSet;
use indexmap::{IndexMap, IndexSet};
use serde::{de::Visitor, Deserialize, Deserializer, Serialize};

use super::NodeKey;
use crate::{
    key_source::KeySource,
    lasso::Spur,
    node_targets::NodeTargets,
    set::{MaskBit, MASK_PREFIX_LEN},
    state::{AgentId, HeightRequest, InternedId, NetworkId, NodeState},
    INTERN,
};

/// A document describing the node infrastructure for a test.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct NodesDocument {
    pub name: String,
    pub description: Option<String>,
    /// The network to use for all nodes.
    ///
    /// Determines if /mainnet/ or /testnet/ are used in routes.
    ///
    /// Also determines which parameters/genesis block to use
    pub network: Option<NetworkId>,

    #[serde(default)]
    pub external: IndexMap<NodeKey, ExternalNode>,

    #[serde(default)]
    pub nodes: IndexMap<NodeKey, NodeDoc>,
}

impl NodesDocument {
    pub fn expand_internal_replicas(&self) -> impl Iterator<Item = (NodeKey, NodeDoc)> + '_ {
        self.nodes.iter().flat_map(|(doc_node_key, doc_node)| {
            let num_replicas = doc_node.replicas.map(|r| r.get()).unwrap_or(1);

            // Iterate over the replicas
            (0..num_replicas.min(10000)).map(move |i| {
                let node_key = match num_replicas {
                    // If there is only one replica, use the doc_node_key
                    1 => doc_node_key.to_owned(),
                    // If there are multiple replicas, append the index to the
                    // doc_node_key
                    _ => {
                        let mut node_key = doc_node_key.to_owned();
                        if !node_key.id.is_empty() {
                            node_key.id.push('-');
                        }
                        node_key.id.push_str(&i.to_string());
                        node_key
                    }
                };

                // Replace the key with a new one
                let mut node = doc_node.to_owned();
                node.replicas = None;

                // Update the node's private key
                if let Some(key) = node.key.as_mut() {
                    *key = key.with_index(i);
                }

                (node_key, node)
            })
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalNode {
    // NOTE: these fields must be validated at runtime, because validators require `bft` to be set,
    // and non-validators require `node` to be set
    // rest is required to be a target of the tx-cannon
    pub bft: Option<SocketAddr>,
    pub node: Option<SocketAddr>,
    pub rest: Option<SocketAddr>,
}

/// Impl serde Deserialize ExternalNode but allow for { bft: addr, node: addr,
/// rest: addr} or just `addr`
impl<'de> Deserialize<'de> for ExternalNode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ExternalNodeVisitor;

        impl<'de> Visitor<'de> for ExternalNodeVisitor {
            type Value = ExternalNode;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an ip address or a map of socket addresses")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut bft = None;
                let mut node = None;
                let mut rest = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "bft" => {
                            bft = Some(map.next_value()?);
                        }
                        "node" => {
                            node = Some(map.next_value()?);
                        }
                        "rest" => {
                            rest = Some(map.next_value()?);
                        }
                        _ => {
                            return Err(serde::de::Error::unknown_field(
                                &key,
                                &["bft", "node", "rest"],
                            ));
                        }
                    }
                }

                Ok(ExternalNode { bft, node, rest })
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let ip: IpAddr = v.parse().map_err(E::custom)?;
                Ok(ExternalNode {
                    bft: Some(SocketAddr::new(ip, 5000)),
                    node: Some(SocketAddr::new(ip, 4130)),
                    rest: Some(SocketAddr::new(ip, 3030)),
                })
            }
        }

        deserializer.deserialize_any(ExternalNodeVisitor)
    }
}

// zander forgive me -isaac
fn please_be_online() -> bool {
    true
}

/// Parse the labels as strings, but intern them on load
pub fn deser_label<'de, D>(deserializer: D) -> Result<IndexSet<Spur>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let labels = Vec::<String>::deserialize(deserializer)?;
    Ok(labels
        .into_iter()
        .map(|label| INTERN.get_or_intern(label))
        .collect())
}

fn ser_label<S>(labels: &IndexSet<Spur>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let labels: Vec<&str> = labels.iter().map(|key| INTERN.resolve(key)).collect();
    labels.serialize(serializer)
}

/// A node in the environment spec
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct NodeDoc {
    /// When true, the node will be started
    #[serde(default = "please_be_online")]
    pub online: bool,
    /// When specified, creates a group of nodes, all with the same
    /// configuration.
    #[serde(default)]
    pub replicas: Option<NonZeroUsize>,
    /// The private key to start the node with.
    #[serde(default)]
    pub key: Option<KeySource>,
    /// Height of ledger to inherit.
    ///
    /// * When null, a ledger is created when the node is started.
    /// * When zero, the ledger is empty and only the genesis block is
    ///   inherited.
    #[serde(default)]
    pub height: HeightRequest,

    /// When specified, agents must have these labels
    #[serde(
        default,
        deserialize_with = "deser_label",
        serialize_with = "ser_label"
    )]
    pub labels: IndexSet<Spur>,

    /// When specified, an agent must have this id. Overrides the labels field.
    #[serde(default)]
    pub agent: Option<AgentId>,

    /// List of validators for the node to connect to
    #[serde(default)]
    pub validators: NodeTargets,

    /// List of peers for the node to connect to
    #[serde(default)]
    pub peers: NodeTargets,

    /// Environment variables to inject into the snarkOS process.
    #[serde(default)]
    pub env: IndexMap<String, String>,

    /// The id of the binary for this node to use, uses "default" by default
    #[serde(default)]
    pub binary: Option<InternedId>,
}

impl NodeDoc {
    pub fn into_state(&self, node_key: NodeKey) -> NodeState {
        NodeState {
            node_key,
            private_key: Default::default(),
            height: (0, self.height),
            online: self.online,
            env: self.env.clone(),
            binary: self.binary,

            // these are resolved later
            validators: Default::default(),
            peers: Default::default(),
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
