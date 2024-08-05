use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnarkOSStatus {
    /// Initial node status
    Starting,
    /// Node is loading the ledger
    LedgerLoading,
    /// Failure to load the ledger
    LedgerFailure(String),
    /// Node is running
    Started,
    /// Node crashed
    Halted(Option<String>),
}

/// Messages from snarkos to the agent, containing information about the status
/// of the node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SnarkOSBlockInfo {
    pub height: u32,
    pub state_root: String,
    pub block_hash: String,
    pub block_timestamp: i64,
}
