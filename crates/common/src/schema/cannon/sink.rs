use serde::{Deserialize, Serialize};

use crate::{node_targets::NodeTargets, state::TxPipeId};

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
    pub target: Option<NodeTargets>,
    /// Number of attempts to broadcast a transaction to the target
    /// should the transaction not make it into the next block. This
    /// is helpful for mitigating ghost transactions.
    ///
    /// 0 means no additional tries, None means infinite tries.
    #[serde(default)]
    pub broadcast_attempts: Option<u32>,
    /// Time to wait between broadcast attempts
    #[serde(default = "TxSink::default_retry_timeout")]
    pub broadcast_timeout: u32,
    /// Number of attempts to authorize a transaction before giving up
    ///
    /// 0 means no additional tries, None means infinite tries.
    #[serde(default)]
    pub authorize_attempts: Option<u32>,
    /// Time to wait before re-trying to authorize a transaction
    #[serde(default = "TxSink::default_retry_timeout")]
    pub authorize_timeout: u32,
}

impl TxSink {
    pub fn default_retry_timeout() -> u32 {
        60
    }
}
