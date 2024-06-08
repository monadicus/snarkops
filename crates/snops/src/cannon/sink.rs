use serde::{Deserialize, Serialize};
use snops_common::state::TxPipeId;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", untagged)]
pub enum TxSink {
    /// Write transactions to a file
    #[serde(rename_all = "kebab-case")]
    Record {
        /// filename for the recording txs list
        file_name: TxPipeId,
    },
    /// Send transactions to nodes in a env
    #[serde(rename_all = "kebab-case")]
    RealTime {
        /// The nodes to send transactions to
        ///
        /// Requires cannon to have an associated env_id
        target: snops_common::node_targets::NodeTargets,
    },
}
