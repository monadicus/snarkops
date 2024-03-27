use std::{future, time::Duration};

use serde::Deserialize;

use crate::schema::NodeTargets;

#[derive(Clone, Debug, Deserialize)]
pub enum TxSink {
    /// Write transactions to a file
    Record {
        /// filename for the recording txs list
        name: String,
    },
    //// Write transactions to a ledger query service
    // AoTAppend {
    //     // information for running .. another ledger service
    //     // solely for appending blocks to a ledger...
    //     // storage_id: usize,
    //     // port: u16,
    //     /// Number of transactions per block
    //     tx_per_block: u32,
    // },
    /// Send transactions to nodes in a env
    RealTime {
        /// The nodes to send transactions to
        ///
        /// Requires cannon to have an associated env_id
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
    pub fn timer(&self, count: usize) -> Timer {
        match self {
            TxSink::Record { .. } => Timer {
                state: TimerState::Active(0),
                count,
                burst_rate: Duration::from_secs(1),
                burst_size: 1,
                fire_rate: Duration::ZERO,
            },
            TxSink::RealTime {
                burst_delay_ms,
                tx_per_burst,
                tx_delay_ms,
                ..
            } => Timer {
                state: TimerState::Active(*tx_per_burst as usize),
                count,
                burst_rate: Duration::from_millis(*burst_delay_ms as u64),
                burst_size: *tx_per_burst,
                fire_rate: Duration::from_millis(*tx_delay_ms as u64),
            },
        }
    }
}

pub struct Timer {
    count: usize,
    burst_rate: Duration,
    burst_size: u32,
    fire_rate: Duration,
    state: TimerState,
}

enum TimerState {
    /// wait the `fire_rate` duration
    Active(usize),
    /// wait the `burst_rate` duration
    Waiting,
    /// wait forever
    Done,
}

impl Timer {
    /*

    example for burst 6, size 3,
    wait is `=`, active is `-,` fire is `>`
    [======>-->-->======>-->-->======]


     */

    pub async fn next(&mut self) {
        self.state = match self.state {
            TimerState::Active(remaining) => {
                tokio::time::sleep(self.fire_rate).await;

                // we reach this point by having waited before, so we remove one
                match remaining.saturating_sub(1) {
                    // if the count was 1, wait the full burst time
                    0 => TimerState::Waiting,
                    // if the count was nonzero, wait at least 1 more fire time
                    n => TimerState::Active(n),
                }
            }
            TimerState::Waiting => {
                self.count.saturating_sub(1);
                tokio::time::sleep(self.burst_rate).await;
                match self.count {
                    // if count is empty, the next sleep will be permanent
                    0 => TimerState::Done,

                    _ => match self.burst_size {
                        // if the burst size is 0, do a full burst wait
                        0 => TimerState::Waiting,
                        // if the burst size is nonzero, wait for the shorter burst latency
                        _ => TimerState::Active(self.burst_size as usize),
                    },
                }
            }
            TimerState::Done => future::pending().await,
        };
    }
}
