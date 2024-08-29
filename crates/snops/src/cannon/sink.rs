use serde::{Deserialize, Serialize};
use snops_common::state::TxPipeId;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TxSink {
    #[serde(default)]
    /// filename to write transactions to
    pub file_name: Option<TxPipeId>,
    /// Send transactions to nodes in a env
    /// The nodes to send transactions to
    ///
    /// Requires cannon to have an associated env_id
    #[serde(default)]
    pub target: Option<snops_common::node_targets::NodeTargets>,
    /// Number of attempts to broadcast a transaction to the target
    /// should the transaction not make it into the next block. This
    /// is helpful for mitigating ghost transactions.
    ///
    /// None means no tries, 0 means infinite tries.
    #[serde(default)]
    pub broadcast_attempts: Option<u32>,
}
