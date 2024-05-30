use serde::{Deserialize, Serialize};

use crate::{
    key_source::KeySource,
    node_targets::NodeTargets,
    state::{CannonId, DocHeightRequest},
};

#[derive(Deserialize, Serialize, Clone)]
pub struct WithTargets<T = ()> {
    pub nodes: NodeTargets,
    #[serde(flatten)]
    pub data: T,
}

impl From<NodeTargets> for WithTargets {
    fn from(nodes: NodeTargets) -> Self {
        Self { nodes, data: () }
    }
}

impl<T: Serialize> From<T> for WithTargets<T> {
    fn from(data: T) -> Self {
        Self {
            nodes: NodeTargets::None,
            data,
        }
    }
}

impl<T: Serialize> From<(NodeTargets, T)> for WithTargets<T> {
    fn from((nodes, data): (NodeTargets, T)) -> Self {
        Self { nodes, data }
    }
}

fn committee_0_key() -> KeySource {
    KeySource::Committee(Some(0))
}

fn credits_aleo() -> String {
    "credits.aleo".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExecuteAction {
    /// The private key to use for the transaction. If not provided, the
    /// transaction will be signed with the committee member 0's key.
    #[serde(default = "committee_0_key")]
    pub private_key: KeySource,
    /// The program to execute. Defaults to `credits.aleo`
    #[serde(default = "credits_aleo")]
    pub program: String,
    /// The function to call
    pub function: String,
    /// The cannon id of who to execute the transaction
    #[serde(default)]
    pub cannon: CannonId,
    /// The inputs to the function
    pub inputs: Vec<AleoValue>,
    /// The optional priority fee
    #[serde(default)]
    pub priority_fee: Option<u64>,
    /// The optional fee record for a private fee
    #[serde(default)]
    pub fee_record: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AleoValue {
    // Public keys
    Key(KeySource),
    // Other values (u8, fields, etc.)
    Other(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reconfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub online: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<DocHeightRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peers: Option<NodeTargets>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validators: Option<NodeTargets>,
}
