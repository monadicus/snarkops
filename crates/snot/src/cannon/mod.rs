pub mod router;
pub mod sink;
pub mod source;

use std::sync::Arc;

use anyhow::{bail, ensure, Result};

use tokio::{
    sync::{mpsc::UnboundedSender, Mutex as AsyncMutex},
    task::AbortHandle,
};
use tracing::warn;

use crate::{cannon::source::LedgerQueryService, state::GlobalState};

use self::{sink::TxSink, source::TxSource};

/*

STEP ONE
cannon transaction source: (GEN OR PLAYBACK)
- AOT: storage file
- REALTIME: generate executions from available agents?? via rpc


STEP 2
cannon query source:
/cannon/<id>/mainnet/latest/stateRoot forwards to one of the following:
- REALTIME-(GEN|PLAYBACK): (test_id, node-key) with a rest ports Client/Validator only
- AOT-GEN: ledger service locally (file mode)
- AOT-PLAYBACK: n/a

STEP 3
cannon broadcast ALWAYS HITS control plane at
/cannon/<id>/mainnet/transaction/broadcast
cannon TX OUTPUT pointing at
- REALTIME: (test_id, node-key)
- AOT: file


cannon rate
cannon buffer size
burst mode??

*/

/// Transaction cannon
#[derive(Debug)]
pub struct TestCannon {
    // a copy of the global state
    global_state: Arc<GlobalState>,

    source: TxSource,
    sink: TxSink,

    /// channel to send transactions to the the task
    tx_sender: UnboundedSender<String>,

    /// The test_id associated with this cannon.
    /// To point at an external node, create a topology with external node
    test_id: Option<usize>,

    // TODO: run the actual cannon in this task
    task: AsyncMutex<AbortHandle>,
}

impl TestCannon {
    pub fn new(
        global_state: Arc<GlobalState>,
        source: TxSource,
        sink: TxSink,
        test_id: Option<usize>,
    ) -> Result<Self> {
        ensure!(
            (source.needs_test_id() || sink.needs_test_id()) != test_id.is_some(),
            "Test ID must be provided if either source or sink requires it"
        );

        // TODO: maybe Arc<TxSource>, then pass it to this task

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let tx_sender = tx.clone();

        let handle = tokio::spawn(async move {
            // TODO: write tx to sink at desired rate
            let _tx = rx.recv().await;

            std::future::pending::<()>().await
        });

        Ok(Self {
            global_state,
            source,
            sink,
            test_id,
            tx_sender,
            task: AsyncMutex::new(handle.abort_handle()),
        })
    }

    /// Called by axum to forward /cannon/<id>/mainnet/latest/stateRoot
    /// to the ledger query service's /mainnet/latest/stateRoot
    pub async fn proxy_state_root(&self) -> Result<String> {
        match &self.source {
            TxSource::RealTime { query, .. } => match query {
                LedgerQueryService::Local(qs) => qs.get_state_root().await,
                LedgerQueryService::Node(key) => {
                    // test_id must be Some because LedgerQueryService::Node requires it
                    let Some(agent_id) = self
                        .global_state
                        .get_test_agent(self.test_id.unwrap(), key)
                        .await
                    else {
                        bail!("cannon target agent not found")
                    };

                    let Some(client) = self.global_state.get_client(agent_id).await else {
                        bail!("cannon target agent is offline")
                    };

                    // call client's rpc method to get the state root
                    // this will fail if the client is not running a node
                    client.get_state_root().await
                }
            },
            TxSource::AoTPlayback { .. } => {
                bail!("cannon is configured to playback from file.")
            }
        }
    }

    /// Called by axum to forward /cannon/<id>/mainnet/transaction/broadcast
    /// to the desired sink
    pub fn proxy_broadcast(&self, body: String) -> Result<()> {
        match &self.source {
            TxSource::RealTime { .. } => {
                self.tx_sender.send(body)?;
            }
            TxSource::AoTPlayback { .. } => {
                warn!("cannon received broadcasted transaction in playback mode. ignoring.")
            }
        }
        Ok(())
    }
}
