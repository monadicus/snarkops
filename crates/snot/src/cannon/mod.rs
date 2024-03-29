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
use snot_common::state::AgentPeer;
use tokio::{
    process::Command,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::AbortHandle,
};
use tracing::{info, trace, warn};

use self::{
    file::{TransactionDrain, TransactionSink},
    sink::TxSink,
    source::TxSource,
};
use crate::{
    cannon::{
        sink::Timer,
        source::{ComputeTarget, QueryTarget},
    },
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

pub type Authorization = serde_json::Value;

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
    /// channel to send authorizations to the the task
    auth_sender: UnboundedSender<Authorization>,

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

pub struct CannonReceivers {
    transactions: UnboundedReceiver<String>,
    authorizations: UnboundedReceiver<Authorization>,
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
    ) -> Result<(Self, CannonReceivers)> {
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

        let (auth_sender, auth_receiver) = tokio::sync::mpsc::unbounded_channel();

        Ok((
            Self {
                id,
                global_state,
                source,
                sink,
                env: Arc::downgrade(&env),
                tx_sender,
                auth_sender,
                query_port,
                child,
                task: None,
                fired_txs,
                tx_count: count,
            },
            CannonReceivers {
                transactions: tx_receiver,
                authorizations: auth_receiver,
            },
        ))
    }

    pub fn ctx(&self) -> Result<ExecutionContext> {
        Ok(ExecutionContext {
            id: self.id,
            env: self.env.clone(),
            source: self.source.clone(),
            sink: self.sink.clone(),
            fired_txs: self.fired_txs.clone(),
            state: self.global_state.clone(),
            tx_count: self.tx_count,
            tx_sender: self.tx_sender.clone(),
            auth_sender: self.auth_sender.clone(),
        })
    }

    pub async fn spawn_local(&mut self, rx: CannonReceivers) -> Result<()> {
        let ctx = self.ctx()?;

        let handle = tokio::task::spawn(async move { ctx.spawn(rx).await });
        self.task = Some(handle.abort_handle());

        Ok(())
    }

    pub async fn spawn(&mut self, rx: CannonReceivers) -> Result<()> {
        self.ctx()?.spawn(rx).await
    }

    /// Called by axum to forward /cannon/<id>/mainnet/latest/stateRoot
    /// to the ledger query service's /mainnet/latest/stateRoot
    pub async fn proxy_state_root(&self) -> Result<String> {
        match &self.source {
            TxSource::RealTime { query, .. } | TxSource::Listen { query, .. } => match query {
                QueryTarget::Local(qs) => {
                    if let Some(port) = self.query_port {
                        qs.get_state_root(port).await
                    } else {
                        bail!("cannon is missing a query port")
                    }
                }
                QueryTarget::Node(key) => {
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
            TxSource::RealTime { .. } | TxSource::Listen { .. } => {
                self.tx_sender.send(body)?;
            }
            TxSource::Playback { .. } => {
                warn!("cannon received broadcasted transaction in playback mode. ignoring.")
            }
        }
        Ok(())
    }

    /// Called by axum to forward /cannon/<id>/auth to a listen source
    pub fn proxy_auth(&self, body: Authorization) -> Result<()> {
        match &self.source {
            TxSource::Listen { .. } => {
                self.auth_sender.send(body)?;
            }
            TxSource::RealTime { .. } => {
                warn!("cannon received broadcasted transaction in realtime mode. ignoring.")
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
    tx_sender: UnboundedSender<String>,
    auth_sender: UnboundedSender<Authorization>,
}

impl ExecutionContext {
    pub async fn spawn(self, mut rx: CannonReceivers) -> Result<()> {
        let ExecutionContext {
            id: cannon_id,
            env: env_weak,
            source,
            sink,
            fired_txs,
            tx_count,
            state,
            ..
        } = &self;

        let Some(env) = env_weak.upgrade() else {
            bail!("env dropped")
        };
        let env_id = env.id;

        trace!("cannon {env_id}.{cannon_id} spawned");

        // when in playback mode, ensure the drain exists
        let (drain_pipe, query_path) = match &source {
            TxSource::Playback { file_name: name } => {
                let pipe = env.tx_pipe.drains.get(name).cloned();
                ensure!(pipe.is_some(), "transaction drain not found: {name}");
                (pipe, None)
            }
            TxSource::RealTime { compute, .. } | TxSource::Listen { compute, .. } => {
                let suffix = format!("/api/v1/env/{}/cannons/{cannon_id}", env.id);
                let query = match compute {
                    // agents already know the host of the control plane
                    ComputeTarget::Agent => suffix,
                    // demox needs to locate it
                    ComputeTarget::Demox { .. } => {
                        let Some(host) = get_host(state).await else {
                            bail!("no --host configured for demox based cannon");
                        };
                        format!("http://{host}:{}{suffix}", state.cli.port)
                    }
                };
                trace!("cannon {env_id}.{cannon_id} using realtime query {query}");
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

        // build a timer that keeps track of the expected sink speed
        // if the source is listen, the sink's rate is ignored
        let mut timer = matches!(source, TxSource::Listen { .. })
            .then(Timer::never)
            .unwrap_or_else(|| sink.timer(*tx_count));

        let mut tx_reqs = FuturesUnordered::new();
        let mut auth_execs = FuturesUnordered::new();
        let mut tx_shots = FuturesUnordered::new();

        loop {
            tokio::select! {
                // ------------------------
                // Work generation
                // ------------------------

                // when the timer resolves, request a new transaction
                _ = timer.next() => {
                    tx_reqs.push(self.request_tx(drain_pipe.clone()));
                }
                // receive authorizations and forward the executions to the compute target
                Some(auth) = rx.authorizations.recv() => {
                    auth_execs.push(self.execute_auth(auth, query_path.clone().unwrap()));
                }
                // receive transactions and forward them to the sink target
                Some(tx) = rx.transactions.recv() => {
                    tx_shots.push(self.fire_tx(sink_pipe.clone(), tx));
                }

                // ------------------------
                // Work results
                // ------------------------

                Some(res) = tx_reqs.next() => {
                    match res {
                        // if the request was successful, continue
                        Ok(true) => {}
                        // if the source is depleted, break the loop
                        Ok(false) => {
                            info!("cannon {env_id}.{cannon_id} source depleted after {} txs", fired_txs.load(std::sync::atomic::Ordering::Relaxed));
                            break;
                        },
                        // if the request failed, undo the timer to allow another transaction to replace the failure
                        Err(e) => {
                            warn!("cannon {env_id}.{cannon_id} transaction task failed: {e}");
                            timer.undo();
                        }
                    }
                },
                Some(res) = auth_execs.next() => {
                    if let Err(e) = res {
                        warn!("cannon {env_id}.{cannon_id} auth execute task failed: {e}");
                        timer.undo();
                    }
                },
                Some(res) = tx_shots.next() => {
                    match res {
                        Ok(()) => {
                            let fired_count = fired_txs.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                            if fired_count >= *tx_count {
                                trace!("cannon {env_id}.{cannon_id} finished firing txs");
                                break;
                            }
                            trace!("cannon {env_id}.{cannon_id} fired {fired_count}/{tx_count} txs");
                        }
                        Err(e) => {
                            warn!("cannon {env_id}.{cannon_id} failed to fire transaction {e}");
                            timer.undo();
                        }
                    }
                },
            }
        }

        Ok(())
    }

    /// Request a new transaction from the context's source
    async fn request_tx(&self, drain_pipe: Option<Arc<TransactionDrain>>) -> Result<bool> {
        match &self.source {
            TxSource::Playback { .. } => {
                // if tx source is playback, read lines from the transaction file
                let Some(transaction) = drain_pipe.unwrap().next()? else {
                    return Ok(false);
                };
                self.tx_sender.send(transaction)?;
                Ok(true)
            }
            TxSource::RealTime { .. } => {
                let Some(env) = self.env.upgrade() else {
                    bail!("env dropped")
                };
                trace!("cannon {}.{} generating authorization...", env.id, self.id);

                let auth = self.source.get_auth(&env)?.run(&env.aot_bin).await?;
                self.auth_sender.send(auth)?;
                Ok(true)
            }
            TxSource::Listen { .. } => {
                unreachable!("listen mode cannot generate transactions")
            }
        }
    }

    /// Execute an authorization on the source's compute target
    async fn execute_auth(&self, auth: Authorization, query_path: String) -> Result<()> {
        match &self.source {
            TxSource::Playback { .. } => {
                unreachable!("playback mode cannot receive authorizations")
            }
            TxSource::RealTime { compute, .. } | TxSource::Listen { compute, .. } => {
                let env = self
                    .env
                    .upgrade()
                    .ok_or_else(|| anyhow::anyhow!("env dropped"))?;
                compute.execute(&self.state, &env, query_path, auth).await
            }
        }
    }

    /// Fire a transaction to the sink
    async fn fire_tx(&self, sink_pipe: Option<Arc<TransactionSink>>, tx: String) -> Result<()> {
        match &self.sink {
            TxSink::Record { .. } => {
                sink_pipe.unwrap().write(&tx)?;
            }
            TxSink::RealTime { target, .. } => {
                let pool = self.state.pool.read().await;
                let nodes = self
                    .env
                    .upgrade()
                    .ok_or_else(|| anyhow::anyhow!("env dropped"))?
                    .matching_nodes(target, &pool, PortType::Rest)
                    .collect::<Vec<_>>();

                if nodes.is_empty() {
                    bail!("no nodes available to broadcast transactions")
                }

                let Some(node) = nodes.get(rand::random::<usize>() % nodes.len()) else {
                    bail!("no nodes available to broadcast transactions")
                };
                match node {
                    AgentPeer::Internal(id, _) => {
                        let Some(client) = pool[id].client_owned() else {
                            bail!("target agent {id} was offline");
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
        Ok(())
    }
}
