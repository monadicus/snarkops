use checkpoint::RetentionPolicy;
use serde::{Deserialize, Serialize};

/// Metadata about a checkpoint file
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CheckpointMeta {
    pub height: u32,
    pub timestamp: i64,
    pub filename: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StorageInfo {
    /// String id of this storage
    pub id: String,
    /// The retention policy used for this storage
    pub retention_policy: Option<RetentionPolicy>,
    /// The available checkpoints in this storage
    pub checkpoints: Vec<CheckpointMeta>,
    /// Whether to persist the ledger
    pub persist: bool,
}
