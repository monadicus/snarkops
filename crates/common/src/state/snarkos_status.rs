use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::LatestBlockInfo;

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

impl SnarkOSStatus {
    pub fn is_started(&self) -> bool {
        matches!(self, SnarkOSStatus::Started)
    }

    pub fn is_stopped(&self) -> bool {
        matches!(
            self,
            SnarkOSStatus::Halted(_) | SnarkOSStatus::LedgerFailure(_)
        )
    }

    pub fn label(&self) -> &'static str {
        match self {
            SnarkOSStatus::Starting => "starting",
            SnarkOSStatus::LedgerLoading => "loading",
            SnarkOSStatus::LedgerFailure(_) => "failure",
            SnarkOSStatus::Started => "started",
            SnarkOSStatus::Halted(_) => "halted",
        }
    }
}

/// Messages from snarkos to the agent, containing information about the status
/// of the node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SnarkOSBlockInfo {
    pub height: u32,
    pub state_root: String,
    pub block_hash: String,
    pub previous_hash: String,
    pub block_timestamp: i64,
}

/// Messages from snarkos to the agent, containing information about the status
/// of the node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SnarkOSLiteBlock {
    pub info: SnarkOSBlockInfo,
    pub transactions: Vec<String>,
}

impl SnarkOSLiteBlock {
    pub fn split(self) -> (LatestBlockInfo, Vec<Arc<str>>) {
        (
            LatestBlockInfo {
                height: self.info.height,
                state_root: self.info.state_root,
                block_hash: self.info.block_hash,
                previous_hash: self.info.previous_hash,
                block_timestamp: self.info.block_timestamp,
                update_time: Utc::now(),
            },
            self.transactions.into_iter().map(Arc::from).collect(),
        )
    }
}
