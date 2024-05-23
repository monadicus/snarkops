pub mod authorized;
pub mod error;
pub mod file;
mod net;
pub mod router;
pub mod sink;
pub mod source;

use std::{
    path::PathBuf,
    process::Stdio,
    sync::{atomic::AtomicUsize, Arc},
};

use futures_util::{stream::FuturesUnordered, StreamExt};
use rand::seq::IteratorRandom;
use snops_common::{
    aot_cmds::error::CommandError,
    constant::{LEDGER_BASE_DIR, SNARKOS_GENESIS_FILE},
    state::{AgentPeer, CannonId, EnvId, StorageId},
};
use tokio::{
    process::Command,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::AbortHandle,
};
use tracing::{info, trace, warn};

use self::{
    error::{CannonError, CannonInstanceError, ExecutionContextError},
    file::{TransactionDrain, TransactionSink},
    sink::TxSink,
    source::TxSource,
};
use crate::{
    cannon::{
        sink::Timer,
        source::{ComputeTarget, QueryTarget},
    },
    env::PortType,
    state::GlobalState,
};

/*

STEP ONE
cannon transaction source: (GEN OR PLAYBACK)
- AOT: storage file
- REALTIME: generate executions from available agents?? via rpc


STEP 2
cannon query source:
/cannon/<id>/<network>/latest/stateRoot forwards to one of the following:
- REALTIME-(GEN|PLAYBACK): (test_id, node-key) with a rest ports Client/Validator only
- AOT-GEN: ledger service locally (file mode)
- AOT-PLAYBACK: n/a

STEP 3
cannon broadcast ALWAYS HITS control plane at
/cannon/<id>/<network>/transaction/broadcast
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
    id: CannonId,
    // a copy of the global state
    global_state: Arc<GlobalState>,

    source: TxSource,
    sink: TxSink,

    /// The test_id/storage associated with this cannon.
    /// To point at an external node, create a topology with external node
    /// To generate ahead-of-time, upload a test with a timeline referencing a
    /// cannon pointing at a file
    env_id: EnvId,

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

    pub(crate) fired_txs: Arc<AtomicUsize>,
    tx_count: Option<usize>,
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
    pub fn new(
        global_state: Arc<GlobalState>,
        id: CannonId,
        (env_id, storage_id, aot_bin): (EnvId, StorageId, &PathBuf),
        source: TxSource,
        sink: TxSink,
        count: Option<usize>,
    ) -> Result<(Self, CannonReceivers), CannonError> {
        let (tx_sender, tx_receiver) = tokio::sync::mpsc::unbounded_channel();
        let query_port = source.get_query_port()?;
        let fired_txs = Arc::new(AtomicUsize::new(0));

        let env = global_state
            .get_env(env_id)
            .ok_or_else(|| ExecutionContextError::EnvDropped(env_id, id))?;
        let storage_path = global_state.storage_path(env.network, storage_id);

        // spawn child process for ledger service if the source is local
        let child = if let Some(port) = query_port {
            // TODO: make a copy of this ledger dir to prevent locks
            let child = Command::new(aot_bin)
                .kill_on_drop(true)
                .env("NETWORK", env.network.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .arg("ledger")
                .arg("-l")
                .arg(storage_path.join(LEDGER_BASE_DIR))
                .arg("-g")
                .arg(storage_path.join(SNARKOS_GENESIS_FILE))
                .arg("query")
                .arg("--port")
                .arg(port.to_string())
                .arg("--bind")
                .arg("127.0.0.1") // only bind to localhost as this is a private process
                .arg("--readonly")
                .spawn()
                .map_err(|e| {
                    CannonError::Command(id, CommandError::action("spawning", "aot ledger", e))
                })?;
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
                env_id,
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

    pub fn ctx(&self) -> Result<ExecutionContext, CannonError> {
        Ok(ExecutionContext {
            id: self.id,
            env_id: self.env_id,
            source: self.source.clone(),
            sink: self.sink.clone(),
            fired_txs: Arc::clone(&self.fired_txs),
            state: Arc::clone(&self.global_state),
            tx_count: self.tx_count,
            tx_sender: self.tx_sender.clone(),
            auth_sender: self.auth_sender.clone(),
        })
    }

    pub fn spawn_local(&mut self, rx: CannonReceivers) -> Result<(), CannonError> {
        let ctx = self.ctx()?;

        let handle = tokio::task::spawn(async move { ctx.spawn(rx).await });
        self.task = Some(handle.abort_handle());

        Ok(())
    }

    pub async fn spawn(&mut self, rx: CannonReceivers) -> Result<(), CannonError> {
        self.ctx()?.spawn(rx).await
    }

    /// Called by axum to forward /cannon/<id>/<network>/latest/stateRoot
    /// to the ledger query service's /<network>/latest/stateRoot
    pub async fn proxy_state_root(&self) -> Result<String, CannonError> {
        match &self.source {
            TxSource::RealTime { query, .. } | TxSource::Listen { query, .. } => match query {
                QueryTarget::Local(qs) => {
                    if let Some(port) = self.query_port {
                        let network = self
                            .global_state
                            .get_env(self.env_id)
                            .ok_or_else(|| ExecutionContextError::EnvDropped(self.env_id, self.id))?
                            .network;
                        qs.get_state_root(network, port).await
                    } else {
                        Err(CannonInstanceError::MissingQueryPort(self.id).into())
                    }
                }
                QueryTarget::Node(key) => {
                    let Some(env) = self.global_state.get_env(self.env_id) else {
                        unreachable!("called from a place where env is present")
                    };

                    // env_id must be Some because LedgerQueryService::Node requires it
                    let Some(agent_id) = env.get_agent_by_key(key) else {
                        return Err(
                            CannonInstanceError::TargetAgentNotFound(self.id, key.clone()).into(),
                        );
                    };

                    let Some(client) = self.global_state.get_client(agent_id) else {
                        return Err(CannonError::TargetAgentOffline(
                            "cannon",
                            self.id,
                            key.to_string(),
                        ));
                    };

                    // call client's rpc method to get the state root
                    // this will fail if the client is not running a node
                    Ok(client.get_state_root().await?)
                }
            },
            TxSource::Playback { .. } => {
                Err(CannonInstanceError::NotConfiguredToPlayback(self.id))?
            }
        }
    }

    /// Called by axum to forward /cannon/<id>/<network>/transaction/broadcast
    /// to the desired sink
    pub fn proxy_broadcast(&self, body: String) -> Result<(), CannonError> {
        match &self.source {
            TxSource::RealTime { .. } | TxSource::Listen { .. } => {
                self.tx_sender
                    .send(body)
                    .map_err(|e| CannonError::SendTxError(self.id, e))?;
            }
            TxSource::Playback { .. } => {
                warn!("cannon received broadcasted transaction in playback mode. ignoring.")
            }
        }
        Ok(())
    }

    /// Called by axum to forward /cannon/<id>/auth to a listen source
    pub fn proxy_auth(&self, body: Authorization) -> Result<(), CannonError> {
        match &self.source {
            TxSource::Listen { .. } => {
                self.auth_sender
                    .send(body)
                    .map_err(|e| CannonError::SendAuthError(self.id, e))?;
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
    id: CannonId,
    /// The environment associated with this cannon
    env_id: EnvId,
    source: TxSource,
    sink: TxSink,
    fired_txs: Arc<AtomicUsize>,
    tx_count: Option<usize>,
    tx_sender: UnboundedSender<String>,
    auth_sender: UnboundedSender<Authorization>,
}

impl ExecutionContext {
    pub async fn spawn(self, mut rx: CannonReceivers) -> Result<(), CannonError> {
        let ExecutionContext {
            id: cannon_id,
            env_id,
            source,
            sink,
            fired_txs,
            tx_count,
            state,
            ..
        } = &self;

        let env = state
            .envs
            .get(env_id)
            .ok_or_else(|| ExecutionContextError::EnvDropped(*env_id, *cannon_id))?;
        let env_id = *env_id;

        trace!("cannon {env_id}.{cannon_id} spawned");

        // when in playback mode, ensure the drain exists
        let (drain_pipe, query_path) = match &source {
            TxSource::Playback { file_name: name } => {
                let pipe = env.tx_pipe.drains.get(name).cloned();
                if pipe.is_none() {
                    return Err(ExecutionContextError::TransactionDrainNotFound(
                        env_id, *cannon_id, *name,
                    )
                    .into());
                }
                (pipe, None)
            }
            TxSource::RealTime { compute, .. } | TxSource::Listen { compute, .. } => {
                let suffix = format!("/api/v1/env/{}/cannons/{cannon_id}", env.id);
                let query = match compute {
                    // agents already know the host of the control plane
                    ComputeTarget::Agent { .. } => suffix,
                    // demox needs to locate it
                    ComputeTarget::Demox { .. } => {
                        let host = state
                            .cli
                            .hostname
                            .as_ref()
                            .ok_or(ExecutionContextError::NoHostnameConfigured)?;
                        format!("{host}:{}{suffix}", state.cli.port)
                    }
                };
                trace!("cannon {env_id}.{cannon_id} using realtime query {query}");
                (None, Some(query))
            }
        };

        let sink_pipe = match &sink {
            TxSink::Record { file_name, .. } => {
                let pipe = env.tx_pipe.sinks.get(file_name).cloned();
                if pipe.is_none() {
                    return Err(ExecutionContextError::TransactionSinkNotFound(
                        env_id, *cannon_id, *file_name,
                    )
                    .into());
                }
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
                            if let Some(tx_count) = tx_count {
                                if fired_count >= *tx_count {
                                    trace!("cannon {env_id}.{cannon_id} finished firing txs");
                                    break;
                                }
                                trace!("cannon {env_id}.{cannon_id} fired {fired_count}/{tx_count} txs");
                            } else {
                                trace!("cannon {env_id}.{cannon_id} fired {fired_count} txs");
                            }
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
    async fn request_tx(
        &self,
        drain_pipe: Option<Arc<TransactionDrain>>,
    ) -> Result<bool, CannonError> {
        match &self.source {
            TxSource::Playback { .. } => {
                let Some(drain_pipe) = drain_pipe else {
                    return Ok(false);
                };

                let tx = drain_pipe.next()?;
                drain_pipe.write_persistence(self).await;

                // if tx source is playback, read lines from the transaction file
                let Some(transaction) = tx else {
                    return Ok(false);
                };

                self.tx_sender
                    .send(transaction)
                    .map_err(|e| CannonError::SendTxError(self.id, e))?;
                Ok(true)
            }
            TxSource::RealTime { .. } => {
                let Some(env) = self.state.get_env(self.env_id) else {
                    return Err(ExecutionContextError::EnvDropped(self.env_id, self.id).into());
                };
                trace!("cannon {}.{} generating authorization...", env.id, self.id);

                let auths = self
                    .source
                    .get_auth(&env)?
                    .run(&env.aot_bin, env.network)
                    .await?;
                self.auth_sender
                    .send(auths)
                    .map_err(|e| CannonError::SendAuthError(self.id, e))?;
                Ok(true)
            }
            TxSource::Listen { .. } => {
                unreachable!("listen mode cannot generate transactions")
            }
        }
    }

    /// Execute an authorization on the source's compute target
    async fn execute_auth(
        &self,
        auth: Authorization,
        query_path: String,
    ) -> Result<(), CannonError> {
        match &self.source {
            TxSource::Playback { .. } => {
                unreachable!("playback mode cannot receive authorizations")
            }
            TxSource::RealTime { compute, .. } | TxSource::Listen { compute, .. } => {
                let env = self
                    .state
                    .get_env(self.env_id)
                    .ok_or_else(|| ExecutionContextError::EnvDropped(self.env_id, self.id))?;
                compute.execute(&self.state, &env, query_path, auth).await
            }
        }
    }

    /// Fire a transaction to the sink
    async fn fire_tx(
        &self,
        sink_pipe: Option<Arc<TransactionSink>>,
        tx: String,
    ) -> Result<(), CannonError> {
        match &self.sink {
            TxSink::Record { .. } => {
                sink_pipe.unwrap().write(&tx)?;
            }
            TxSink::RealTime { target, .. } => {
                let nodes = self
                    .state
                    .get_env(self.env_id)
                    .ok_or_else(|| ExecutionContextError::EnvDropped(self.env_id, self.id))?
                    .matching_nodes(target, &self.state.pool, PortType::Rest)
                    .collect::<Vec<_>>();

                if nodes.is_empty() {
                    return Err(ExecutionContextError::NoAvailableAgents(
                        "to broadcast transactions",
                        self.id,
                    )
                    .into());
                }

                let Some(node) = nodes.iter().choose(&mut rand::thread_rng()) else {
                    return Err(ExecutionContextError::NoAvailableAgents(
                        "to broadcast transactions",
                        self.id,
                    )
                    .into());
                };
                match node {
                    AgentPeer::Internal(id, _) => {
                        let Some(client) = self.state.get_client(*id) else {
                            return Err(CannonError::TargetAgentOffline(
                                "exec ctx",
                                self.id,
                                id.to_string(),
                            ));
                        };

                        client.broadcast_tx(tx).await?;
                    }
                    AgentPeer::External(addr) => {
                        let network = self
                            .state
                            .get_env(self.env_id)
                            .ok_or_else(|| ExecutionContextError::EnvDropped(self.env_id, self.id))?
                            .network;
                        let url = format!("http://{addr}/{network}/transaction/broadcast");
                        let req = reqwest::Client::new()
                            .post(url)
                            .header("Content-Type", "application/json")
                            .body(tx)
                            .send()
                            .await
                            .map_err(|e| ExecutionContextError::BroadcastRequest(self.id, e))?;
                        if !req.status().is_success() {
                            // TODO maybe get response text?
                            Err(ExecutionContextError::Broadcast(
                                self.id,
                                req.status().to_string(),
                            ))?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
