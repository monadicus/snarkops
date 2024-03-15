use indexmap::IndexMap;
use serde::Deserialize;

/// A document describing the node infrastructure for a test.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    // TODO: is there a way to deserialize whether or not this is a client/validator by its name?
    pub nodes: IndexMap<String, Node>,
}

// TODO: could use some more clarification on some of these fields
/// A node in the testing infrastructure.
#[derive(Deserialize, Debug, Clone)]
pub struct Node {
    /// When specified, creates a group of nodes, all with the same
    /// configuration. A a a a a a a a a
    pub replicas: Option<usize>,
    /// The private key to start the node with. When unspecified, a random
    /// private key is generated at runtime.
    pub key: Option<String>,
    /// Height of ledger to inherit.
    ///
    /// * When null, a ledger is created when the node is started.
    /// * When zero, the ledger is empty and only the genesis block is
    ///   inherited.
    pub height: Option<usize>,
    pub validators: Option<Vec<String>>,
    pub peers: Option<Vec<String>>,
}
