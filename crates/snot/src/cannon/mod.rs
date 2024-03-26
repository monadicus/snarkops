pub mod authorized;
pub mod fs_drain;
mod net;
pub mod router;
pub mod sink;
pub mod source;

use std::{
    collections::{HashSet, VecDeque},
    process::Stdio,
    sync::{atomic::AtomicUsize, Arc, Weak},
};

use anyhow::{bail, ensure, Result};
use serde_json::json;
use tokio::{
    process::Command,
    sync::{mpsc::UnboundedSender, Mutex as AsyncMutex, OnceCell},
    task::{AbortHandle, JoinHandle},
};
use tracing::warn;

use crate::{
    cannon::source::{ComputeTarget, LedgerQueryService},
    state::GlobalState,
    testing::Environment,
};

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
    env: Weak<Environment>,

    /// Local query service port. Only present if the TxSource uses a local query source.
    query_port: Option<u16>,

    // TODO: run the actual cannon in this task
    task: AsyncMutex<AbortHandle>,

    /// channel to send transactions to the the task
    tx_sender: UnboundedSender<String>,
    fired_txs: AtomicUsize,
}

async fn get_host(state: &GlobalState) -> Option<String> {
    static ONCE: OnceCell<Option<String>> = OnceCell::const_new();
    match state.cli.hostname.as_ref() {
        Some(host) => Some(host.to_owned()),
        None => ONCE
            .get_or_init(|| async {
                let sources: external_ip::Sources = external_ip::get_http_sources();
                let consensus = external_ip::ConsensusBuilder::new()
                    .add_sources(sources)
                    .build();
                consensus.get_consensus().await.map(|a| a.to_string())
            })
            .await
            .to_owned(),
    }
}

impl CannonInstance {
    /// Create a new active transaction cannon
    /// with the given source and sink.
    ///
    /// Locks the global state's tests and storage for reading.
    pub async fn new(
        global_state: Arc<GlobalState>,
        cannon_id: usize,
        env: Arc<Environment>,
        source: TxSource,
        sink: TxSink,
    ) -> Result<Self> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let tx_sender = tx.clone();

        let query_port = source.get_query_port()?;

        let env2 = env.clone();
        let source2 = source.clone();
        let state = global_state.clone();

        let fired_txs = AtomicUsize::new(0);

        // buffer for transactions
        let tx_queue = VecDeque::<String>::new();

        // spawn child process for ledger service if the source is local
        let mut child = if let Some(_port) = query_port {
            // TODO: make a copy of this ledger dir to prevent locks
            let child = Command::new(&env.aot_bin)
                .kill_on_drop(true)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .arg("ledger")
                .arg("-l")
                .arg(env.storage.path.join("ledger"))
                .arg("-g")
                .arg(env.storage.path.join("genesis.block"))
                .arg("query")
                .spawn()
                .map_err(|e| anyhow::anyhow!("error spawning query service: {e}"))?;
            Some(child)
        } else {
            None
        };

        // when in playback mode, ensure the drain exists
        let (drain, query_path) = match &source {
            TxSource::Playback { name } => {
                let drain = env.tx_drains.get(name).cloned();
                ensure!(drain.is_some(), "transaction drain not found: {name}");
                (drain, None)
            }
            TxSource::RealTime { compute, .. } => {
                let suffix = format!("/api/v1/env/{}/cannons/{cannon_id}", env.id);
                let query = match compute {
                    // agents already know the host of the control plane
                    ComputeTarget::AgentPool => suffix,
                    // demox needs to locate it
                    ComputeTarget::Demox { .. } => {
                        let Some(host) = get_host(&state).await else {
                            bail!("no --host configured for demox based cannon");
                        };
                        format!("http://{host}:{}{suffix}", global_state.cli.port)
                    }
                };
                (None, Some(query))
            }
        };

        let handle: JoinHandle<anyhow::Result<_>> = tokio::spawn(async move {
            // effectively make this be the source of pooling requests

            let gen_tx = || async {
                match &source2 {
                    TxSource::Playback { .. } => {
                        // if tx source is playback, read lines from the transaction file
                        let Some(transaction) = drain.unwrap().next()? else {
                            bail!("source out of transactions")
                        };
                        tx.send(transaction)?;
                        Ok(())
                    }
                    TxSource::RealTime { compute, .. } => {
                        // TODO: if source is realtime, generate authorizations and
                        // send them to any available agent

                        let auth = source2.get_auth(&env2)?.run(&env2.aot_bin).await?;
                        match compute {
                            ComputeTarget::AgentPool => {
                                todo!("find an agent, call the .execute_authorization api")
                            }
                            ComputeTarget::Demox { url } => {
                                let _body = json!({
                                    "jsonrpc": "2.0",
                                    "id": 1,
                                    "method": "generateTransaction",
                                    "params": {
                                        "authorization": serde_json::to_string(&auth["authorization"])?,
                                        "fee": serde_json::to_string(&auth["fee"])?,
                                        "url": query_path,
                                        "broadcast": true,
                                    }
                                });

                                todo!("post on {url}")
                            }
                        }
                    }
                }
            };

            // TODO: build a buffer deep enough to satisfy a few seconds of transactions
            // TODO: as the buffer is drained, queue up more generated transactions

            // compare the tx id to an authorization id
            let _pending_txs = HashSet::<String>::new();

            // env2.storage.lookup_keysource_pk(key)

            // TODO: determine the rate that transactions need to be created
            // based on the sink

            if let Some(mut child) = child.take() {
                child.wait().await?;
            }

            Ok(())
        });

        Ok(Self {
            global_state,
            source,
            sink,
            env: Arc::downgrade(&env),
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
                    let Some(env) = self.env.upgrade() else {
                        unreachable!("called from a place where env is present")
                    };

                    // env_id must be Some because LedgerQueryService::Node requires it
                    let Some(agent_id) = env.get_agent_by_key(key) else {
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
