use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use snops_checkpoint::RetentionPolicy;

use crate::{
    binaries::BinaryEntry,
    prelude::StorageId,
    state::{InternedId, LatestBlockInfo, NetworkId},
};

/// Metadata about a checkpoint file
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CheckpointMeta {
    pub height: u32,
    pub timestamp: i64,
    pub filename: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnvInfo {
    pub network: NetworkId,
    pub storage: StorageInfo,
    pub block: Option<LatestBlockInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StorageInfo {
    /// String id of this storage
    pub id: StorageId,
    /// The retention policy used for this storage
    pub retention_policy: Option<RetentionPolicy>,
    /// The available checkpoints in this storage
    pub checkpoints: Vec<CheckpointMeta>,
    /// Whether to persist the ledger
    pub persist: bool,
    /// Version identifier for this ledger
    pub version: u16,
    /// Whether to use the network's native genesis block
    pub native_genesis: bool,
    /// A map of the snarkos binary ids to a potential download url (when None,
    /// download from the control plane)
    pub binaries: IndexMap<InternedId, BinaryEntry>,
}
