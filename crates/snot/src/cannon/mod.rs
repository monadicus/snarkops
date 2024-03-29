pub mod authorized;
pub mod file;
mod net;
pub mod router;
pub mod sink;
pub mod source;

use std::{
    process::Stdio,
    sync::{atomic::AtomicUsize, Arc, OnceLock, Weak},
};

use anyhow::{bail, ensure, Result};
use futures_util::{stream::FuturesUnordered, StreamExt};
use serde_json::json;
use snot_common::state::AgentPeer;
use tokio::{
    process::Command,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::AbortHandle,
};
use tracing::{debug, trace, warn};

use self::{sink::TxSink, source::TxSource};
use crate::{
    cannon::source::{ComputeTarget, LedgerQueryService},
    env::{Environment, PortType},
    state::GlobalState,
};

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
    id: usize,
    // a copy of the global state
    global_state: Arc<GlobalState>,

    source: TxSource,
    sink: TxSink,

    /// The test_id/storage associated with this cannon.
    /// To point at an external node, create a topology with external node
    /// To generate ahead-of-time, upload a test with a timeline referencing a
    /// cannon pointing at a file
    env: Weak<Environment>,

    /// Local query service port. Only present if the TxSource uses a local
    /// query source.
    query_port: Option<u16>,

    // TODO: run the actual cannon in this task
    pub task: Option<AbortHandle>,

    /// Child process must exist for the duration of the cannon instance.
    /// This value is never used
    #[allow(dead_code)]
    child: Option<tokio::process::Child>,

    /// channel to send transactions to the the task
    tx_sender: UnboundedSender<String>,
    tx_receiver: Option<UnboundedReceiver<String>>,
    fired_txs: Arc<AtomicUsize>,
    tx_count: usize,
}

#[tokio::main]
async fn get_external_ip() -> Option<String> {
    let sources: external_ip::Sources = external_ip::get_http_sources();
    let consensus = external_ip::ConsensusBuilder::new()
        .add_sources(sources)
        .build();
    consensus.get_consensus().await.map(|s| s.to_string())
}

async fn get_host(state: &GlobalState) -> Option<String> {
    static ONCE: OnceLock<Option<String>> = OnceLock::new();
    match state.cli.hostname.as_ref() {
        Some(host) => Some(host.to_owned()),
        None => ONCE.get_or_init(get_external_ip).to_owned(),
    }
}

impl CannonInstance {
    /// Create a new active transaction cannon
    /// with the given source and sink.
    ///
    /// Locks the global state's tests and storage for reading.
    pub async fn new(
        global_state: Arc<GlobalState>,
        id: usize,
        env: Arc<Environment>,
        source: TxSource,
        sink: TxSink,
        count: usize,
    ) -> Result<Self> {
        let (tx_sender, tx_receiver) = tokio::sync::mpsc::unbounded_channel();
        let query_port = source.get_query_port()?;
        let fired_txs = Arc::new(AtomicUsize::new(0));

        // spawn child process for ledger service if the source is local
        let child = if let Some(port) = query_port {
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
                .arg("--port")
                .arg(port.to_string())
                .arg("--bind")
                .arg("127.0.0.1") // only bind to localhost as this is a private process
                .arg("--readonly")
                .spawn()
                .map_err(|e| anyhow::anyhow!("error spawning query service: {e}"))?;
            Some(child)
        } else {
            None
        };

        Ok(Self {
            id,
            global_state,
            source,
            sink,
            env: Arc::downgrade(&env),
            tx_sender,
            tx_receiver: Some(tx_receiver),
            query_port,
            child,
            task: None,
            fired_txs,
            tx_count: count,
        })
    }

    pub fn ctx(&mut self) -> Result<ExecutionContext> {
        let Some(rx) = self.tx_receiver.take() else {
            bail!("cannon already spawned")
        };

        Ok(ExecutionContext {
            id: self.id,
            env: self.env.clone(),
            source: self.source.clone(),
            sink: self.sink.clone(),
            fired_txs: self.fired_txs.clone(),
            state: self.global_state.clone(),
            tx_count: self.tx_count,
            tx: self.tx_sender.clone(),
            rx,
        })
    }

    pub async fn spawn_local(&mut self) -> Result<()> {
        let ctx = self.ctx()?;

        let handle = tokio::task::spawn(async move { ctx.spawn().await });
        self.task = Some(handle.abort_handle());

        Ok(())
    }

    pub async fn spawn(&mut self) -> Result<()> {
        self.ctx()?.spawn().await
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
                debug!("received tx from broadcast path");
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
        debug!("dropping cannon {}", self.id);
        if let Some(handle) = self.task.take() {
            handle.abort();
        }
    }
}

/// Information a transaction cannon needs for execution via spawned task
pub struct ExecutionContext {
    state: Arc<GlobalState>,
    /// The cannon's id
    id: usize,
    /// The environment associated with this cannon
    env: Weak<Environment>,
    source: TxSource,
    sink: TxSink,
    fired_txs: Arc<AtomicUsize>,
    tx_count: usize,
    tx: UnboundedSender<String>,
    rx: UnboundedReceiver<String>,
}

impl ExecutionContext {
    pub async fn spawn(self) -> Result<()> {
        let ExecutionContext {
            id: cannon_id,
            env: env2,
            source,
            sink,
            fired_txs,
            tx_count,
            state,
            tx,
            mut rx,
        } = self;

        let Some(env) = env2.upgrade() else {
            bail!("env dropped")
        };
        let env_id = env.id;

        trace!("cannon {env_id}/{cannon_id} spawned");

        // when in playback mode, ensure the drain exists
        let (drain_pipe, query_path) = match &source {
            TxSource::Playback { file_name: name } => {
                let pipe = env.tx_pipe.drains.get(name).cloned();
                ensure!(pipe.is_some(), "transaction drain not found: {name}");
                (pipe, None)
            }
            TxSource::RealTime { compute, .. } => {
                let suffix = format!("/api/v1/env/{}/cannons/{cannon_id}", env.id);
                let query = match compute {
                    // agents already know the host of the control plane
                    ComputeTarget::Agent => suffix,
                    // demox needs to locate it
                    ComputeTarget::Demox { .. } => {
                        let Some(host) = get_host(&state).await else {
                            bail!("no --host configured for demox based cannon");
                        };
                        format!("http://{host}:{}{suffix}", state.cli.port)
                    }
                };
                trace!("cannon {cannon_id}/{env_id} using realtime query {query}");
                (None, Some(query))
            }
        };

        let sink_pipe = match &sink {
            TxSink::Record { file_name, .. } => {
                let pipe = env.tx_pipe.sinks.get(file_name).cloned();
                ensure!(pipe.is_some(), "transaction sink not found: {file_name}");
                pipe
            }
            _ => None,
        };

        // effectively make this be the source of pooling requests

        let gen_tx = || async {
            match &source {
                TxSource::Playback { .. } => {
                    // if tx source is playback, read lines from the transaction file
                    let Some(transaction) = drain_pipe.unwrap().next()? else {
                        bail!("source out of transactions")
                    };
                    tx.send(transaction)?;
                    Ok(())
                }
                TxSource::RealTime { compute, .. } => {
                    // TODO: if source is realtime, generate authorizations and
                    // send them to any available agent

                    let env = env2
                        .upgrade()
                        .ok_or_else(|| anyhow::anyhow!("env dropped"))?;

                    trace!("cannon {cannon_id}/{env_id} generating authorization...");
                    let auth = source.get_auth(&env)?.run(&env.aot_bin).await?;
                    match compute {
                        ComputeTarget::Agent => {
                            // find a client, mark it as busy
                            let Some((client, _busy)) =
                                state.pool.read().await.values().find_map(|a| {
                                    if !a.is_busy() && a.is_inventory() {
                                        a.client_owned().map(|c| (c, a.make_busy()))
                                    } else {
                                        None
                                    }
                                })
                            else {
                                bail!("no agents available to execute authorization")
                            };

                            // execute the authorization
                            client
                                .execute_authorization(
                                    env2.upgrade()
                                        .ok_or_else(|| anyhow::anyhow!("env dropped"))?
                                        .id,
                                    query_path.unwrap(),
                                    serde_json::to_string(&auth)?,
                                )
                                .await?;

                            Ok(())
                        }
                        ComputeTarget::Demox { url } => {
                            let _body = json!({
                                "jsonrpc": "2.0",
                                "id": 1,
                                "method": "generateTransaction",
                                "params": {
                                    "authorization": serde_json::to_string(&auth["authorization"])?,
                                    "fee": serde_json::to_string(&auth["fee"])?,
                                    "url": query_path.unwrap(),
                                    "broadcast": true,
                                }
                            });

                            todo!("post on {url}")
                        }
                    }
                }
            }
        };

        // build a timer that keeps track of the expected sink speed
        let mut timer = sink.timer(tx_count);

        let mut container = FuturesUnordered::new();

        loop {
            tokio::select! {
                Some(res) = container.next() => {
                    if let Err(e) = res {
                        timer.undo();
                        warn!("transaction gen failed: {e}");
                    }
                },
                _ = timer.next() => {
                    // queue a new transaction
                    container.push(gen_tx.clone()());
                }
                Some(tx) = rx.recv() => {
                    let fired_count = fired_txs.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    match &sink {
                        TxSink::Record { .. } => {
                            debug!("writing tx to file");
                            sink_pipe.clone().unwrap().write(&tx)?;
                        }
                        TxSink::RealTime { target, .. } => {
                            let pool = state.pool.read().await;
                            let nodes = env2.upgrade().ok_or_else(|| anyhow::anyhow!("env dropped"))?
                                .matching_nodes(target, &pool, PortType::Rest)
                                .collect::<Vec<_>>();
                            let Some(node) = nodes.get(rand::random::<usize>() % nodes.len()) else {
                                bail!("no nodes available to broadcast transactions")
                            };
                            match node {
                                AgentPeer::Internal(id, _) => {
                                    let Some(client) = pool[id].client_owned() else {
                                        bail!("target node was offline");
                                    };

                                    client.broadcast_tx(tx).await?;
                                }
                                AgentPeer::External(addr) => {
                                    let url = format!("http://{addr}/mainnet/transaction/broadcast");
                                    ensure!(
                                        reqwest::Client::new()
                                            .post(url)
                                            .header("Content-Type", "application/json")
                                            .body(tx)
                                            .send()
                                            .await?
                                            .status()
                                            .is_success(),
                                        "failed to post transaction to external target node {addr}"
                                    );
                                }
                            }
                        }
                    }

                    if fired_count >= tx_count {
                        debug!("finished firing txs");
                        break;
                    }
                    debug!("fired {fired_count}/{tx_count} txs");
                }
            }
        }

        Ok(())
    }
}
