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
pub struct LatestNodeInfo {
    pub height: u32,
    pub state_root: String,
    pub block_hash: String,
    pub block_timestamp: i64,
    pub block_time: u32,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransferStatus {
    /// The time the transfer started (relative to the agent's startup time)
    pub started_at: u32,
    /// Amount of data transferred in bytes
    pub downloaded_bytes: u64,
    /// Total amount of data to be transferred in bytes
    pub total_bytes: u64,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentStatus {
    /// Version of the agent binary
    pub agent_version: String,
    /// The latest node info
    pub node_info: Option<LatestNodeInfo>,
    /// The status of the node
    pub node_status: NodeStatus,
    /// The number of seconds since this agent was started
    pub online_secs: u64,
    /// The number of seconds since this agent was last connected to the control
    /// plane
    pub connected_secs: u64,
    /// A map of transfers in progress
    pub transfers: IndexMap<String, TransferStatus>,
}
