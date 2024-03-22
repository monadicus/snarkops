use std::net::SocketAddr;

use indexmap::IndexMap;
use serde::Deserialize;
use snot_common::state::{HeightRequest, NodeState, NodeType};

use super::{NodeKey, NodeTargets};

/// A document describing the node infrastructure for a test.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
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

// TODO: could use some more clarification on some of these fields
/// A node in the testing infrastructure.
#[derive(Deserialize, Debug, Clone)]
pub struct Node {
    /// When specified, creates a group of nodes, all with the same
    /// configuration.
    pub replicas: Option<usize>,
    /// The private key to start the node with. When unspecified, a random
    /// private key is generated at runtime.
    pub key: Option<String>,
    /// The storage ID to use when starting the node.
    pub storage: String,
    /// Height of ledger to inherit.
    ///
    /// * When null, a ledger is created when the node is started.
    /// * When zero, the ledger is empty and only the genesis block is
    ///   inherited.
    pub height: Option<usize>,

    #[serde(default)]
    pub validators: NodeTargets,
    #[serde(default)]
    pub peers: NodeTargets,
}

impl Node {
    pub fn into_state(&self, ty: NodeType) -> NodeState {
        NodeState {
            ty,
            private_key: self.key.clone().unwrap_or_default(),

            // TODO
            height: (0, HeightRequest::Top),

            // TODO: should this be online?
            online: true,
            // TODO: resolve validators
            validators: vec![],
            // TODO: resolve peers
            peers: vec![],
        }
    }
}
