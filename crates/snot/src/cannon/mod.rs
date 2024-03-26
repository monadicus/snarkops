mod net;
pub mod router;
pub mod sink;
pub mod source;

use std::{
    collections::HashSet,
    sync::{atomic::AtomicU32, Arc},
};

use anyhow::{bail, Result};

use tokio::{
    sync::{mpsc::UnboundedSender, Mutex as AsyncMutex},
    task::AbortHandle,
};
use tracing::warn;

use crate::{cannon::source::LedgerQueryService, state::GlobalState, testing::Environment};

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

/// Transaction cannon state
/// using the `TxSource` and `TxSink` for configuration.
#[derive(Debug)]
pub struct CannonInstance {
    // a copy of the global state
    global_state: Arc<GlobalState>,

    source: TxSource,
    sink: TxSink,

    /// The test_id/storage associated with this cannon.
    /// To point at an external node, create a topology with external node
    /// To generate ahead-of-time, upload a test with a timeline referencing a
    /// cannon pointing at a file
    env: Arc<Environment>,

    /// Local query service port. Only present if the TxSource uses a local query source.
    query_port: Option<u16>,

    // TODO: run the actual cannon in this task
    task: AsyncMutex<AbortHandle>,

    /// channel to send transactions to the the task
    tx_sender: UnboundedSender<String>,
    fired_txs: AtomicU32,
}

impl CannonInstance {
    /// Create a new active transaction cannon
    /// with the given source and sink.
    ///
    /// Locks the global state's tests and storage for reading.
    pub async fn new(
        global_state: Arc<GlobalState>,
        source: TxSource,
        sink: TxSink,
        test_id: usize,
    ) -> Result<Self> {
        // mapping with async is ugly and blocking_read is scary
        let env = {
            let Some(env) = global_state.envs.read().await.get(&test_id).cloned() else {
                bail!("test {test_id} not found")
            };

            env
        };
        let env2 = env.clone();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let tx_sender = tx.clone();

        let query_port = source.get_query_port()?;

        let fired_txs = AtomicU32::new(0);

        let handle = tokio::spawn(async move {
            // TODO: write tx to sink at desired rate
            let _tx = rx.recv().await;

            // TODO: if a sink or a source uses node_keys or storage
            // env will be used
            println!("{}", env2.storage.id);

            // compare the tx id to an authorization id
            let _pending_txs = HashSet::<String>::new();

            // TODO: if a local query service exists, spawn it here
            // kill on drop

            // TODO: determine the rate that transactions need to be created
            // based on the sink

            // TODO: if source is realtime, generate authorizations and
            // send them to any available agent

            std::future::pending::<()>().await
        });

        Ok(Self {
            global_state,
            source,
            sink,
            env,
            tx_sender,
            query_port,
            task: AsyncMutex::new(handle.abort_handle()),
            fired_txs,
        })
    }

    /// Called by axum to forward /cannon/<id>/mainnet/latest/stateRoot
    /// to the ledger query service's /mainnet/latest/stateRoot
    pub async fn proxy_state_root(&self) -> Result<String> {
        match &self.source {
            TxSource::RealTime { query, .. } => match query {
                LedgerQueryService::Local(qs) => {
                    if let Some(port) = self.query_port {
                        qs.get_state_root(port).await
                    } else {
                        bail!("cannon is missing a query port")
                    }
                }
                LedgerQueryService::Node(key) => {
                    // env_id must be Some because LedgerQueryService::Node requires it
                    let Some(agent_id) = self.env.get_agent_by_key(key) else {
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
            TxSource::Playback { .. } => {
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
            TxSource::Playback { .. } => {
                warn!("cannon received broadcasted transaction in playback mode. ignoring.")
            }
        }
        Ok(())
    }
}

impl Drop for CannonInstance {
    fn drop(&mut self) {
        // cancel the task on drop
        self.task.blocking_lock().abort();
    }
}
