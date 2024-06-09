use chrono::{DateTime, Utc};
use indexmap::IndexMap;

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NodeStatus {
    /// The last known status of the node is unknown
    #[default]
    Unknown,
    /// The node can be started and is not currently running
    Standby,
    /// The node waiting on other tasks to complete before starting
    PendingStart,
    /// The node is starting up and not yet operational
    Starting,
    /// The node is online and operational
    Online,
    /// The node is online and unresponsive
    Unresponsive,
    /// The node was online and is in the process of shutting down
    Stopping,
    /// The node has been stopped and some extra time is needed before it can be
    /// started again
    LedgerWriting,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct LatestBlockInfo {
    pub height: u32,
    /// Current block's state root
    pub state_root: String,
    pub block_hash: String,
    /// Timestamp of the block
    pub block_timestamp: i64,
    /// Time this block info was updated
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
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
    pub interruption: Option<String>,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
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
    pub transfers: IndexMap<u32, TransferStatus>,
}
