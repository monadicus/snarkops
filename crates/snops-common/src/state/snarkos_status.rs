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
    Halted,
}

/// Messages from snarkos to the agent, containing information about the status
/// of the node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SnarkOSNotification {
    Status(SnarkOSStatus),
    Block {
        height: u32,
        state_root: String,
        block_hash: String,
        block_timestamp: i64,
    },
}
