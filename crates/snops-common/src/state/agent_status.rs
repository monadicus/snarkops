use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::snarkos_status::SnarkOSStatus;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// The last known status of the node is unknown
    #[default]
    Unknown,
    /// The node can be started and is not currently running
    Standby,
    /// The node waiting on other tasks to complete before starting
    PendingStart,
    /// The node is running
    Running(SnarkOSStatus),
    /// The node has exited with a status code
    Exited(u8),
    /// The node was online and is in the process of shutting down
    Stopping,
    /// The node has been stopped and some extra time is needed before it can be
    /// started again
    LedgerWriting,
}

impl From<SnarkOSStatus> for NodeStatus {
    fn from(status: SnarkOSStatus) -> Self {
        NodeStatus::Running(status)
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LatestBlockInfo {
    pub height: u32,
    pub state_root: String,
    pub block_hash: String,
    pub block_timestamp: i64,
    pub update_time: DateTime<Utc>,
}

/// Age to stop considering blocks for scoring
const MAX_BLOCK_AGE: u32 = 3600;
/// Age to stop considering updates for scoring
const MAX_UPDATE_AGE: u32 = 60;
/// Number of seconds before update time is worth comparing over
///
/// If two infos have the same block time, and they are both within this many
/// seconds, they are considered equal. Any infos older than this time are
/// penalized for being stale.
const UPDATE_AGE_INDIFFERENCE: u32 = 5;

impl LatestBlockInfo {
    /// Ranking function for block info to sort competing nodes by "freshness"
    pub fn score(&self, now: &DateTime<Utc>) -> u32 {
        // a score from 3600 to 0 based on the age of the block (3600 = block this
        // second)
        let block_age_score =
            if let Some(block_time) = DateTime::from_timestamp(self.block_timestamp, 0) {
                // the number of seconds since the block was created
                let block_age = now
                    .signed_duration_since(block_time)
                    .num_seconds()
                    .clamp(0, MAX_BLOCK_AGE as i64);
                MAX_BLOCK_AGE.saturating_sub(block_age as u32)
            } else {
                0
            };

        // the number of seconds since the agent last updated the block info
        let update_age = now
            .signed_duration_since(self.update_time)
            .num_seconds()
            .clamp(0, MAX_UPDATE_AGE as i64);
        // a score from 60 to 0 based on the age of the update (60 = update this
        // second). Ignore the top 5 seconds for indifference between "fresh" agents
        let update_age_score = MAX_UPDATE_AGE
            .saturating_sub(update_age as u32)
            .clamp(0, MAX_UPDATE_AGE - UPDATE_AGE_INDIFFERENCE);

        // prefer blocks that are newer and have been updated more recently
        // never prefer a block that is older than the latest
        (block_age_score * (MAX_UPDATE_AGE >> 1) + update_age_score)
            // Penalize agents that have not been updated in half the max update age
            .saturating_sub(MAX_UPDATE_AGE >> 1)
    }
}

pub type TransferId = u32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransferStatusUpdate {
    /// The transfer has started.
    Start {
        /// A description of the transfer.
        desc: String,
        /// The number of bytes expected to transfer.
        total: u64,
        /// The time the transfer started.
        time: DateTime<Utc>,
    },
    /// The transfer has made progress.
    Progress {
        /// The current number of bytes transferred.
        downloaded: u64,
    },
    /// The transfer has ended.
    End {
        /// An interruption reason, if any.
        interruption: Option<String>,
    },
    /// The transfer has been cleaned up.
    Cleanup,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TransferStatus {
    /// Description of the transfer
    pub desc: String,
    /// The time the transfer started (relative to the agent's startup time)
    pub started_at: DateTime<Utc>,
    /// The time the transfer was last updated (relative to the agent's startup)
    pub updated_at: DateTime<Utc>,
    /// Amount of data transferred in bytes
    pub downloaded_bytes: u64,
    /// Total amount of data to be transferred in bytes
    pub total_bytes: u64,
    /// A transfer interruption reason, if any.
    pub interruption: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    /// Version of the agent binary
    pub agent_version: Option<String>,
    /// The latest block info
    pub block_info: Option<LatestBlockInfo>,
    /// The status of the node
    pub node_status: NodeStatus,
    /// The time the agent was stated
    pub start_time: Option<DateTime<Utc>>,
    /// The time the agent connected to the control plane
    pub connected_time: Option<DateTime<Utc>>,
    /// A map of transfers in progress
    pub transfers: IndexMap<TransferId, TransferStatus>,
}
