use serde::Deserialize;

use crate::schema::NodeTargets;

#[derive(Debug, Deserialize)]
pub enum TxSink {
    /// Write transactions to a file
    AoTRecord {
        /// filename for the recording txs list
        name: String,
    },
    /// Write transactions to a ledger query service
    AoTAppend {
        // information for running .. another ledger service
        // solely for appending blocks to a ledger...
        // storage_id: usize,
        // port: u16,
        /// Number of transactions per block
        tx_per_block: u32,
    },
    /// Send transactions to nodes in a test
    RealTime {
        /// The nodes to send transactions to
        ///
        /// Requires cannon to have an associated test_id
        target: NodeTargets,

        /// How long between each burst of transactions
        burst_delay_ms: u32,
        /// How many transactions to fire off in each burst
        tx_per_burst: u32,
        /// How long between each transaction in a burst
        tx_delay_ms: u32,
    },
}

impl TxSink {
    pub fn needs_test_id(&self) -> bool {
        matches!(self, Self::RealTime { .. })
    }
}
