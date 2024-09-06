pub mod error;
pub mod file;
mod net;
pub mod router;
pub mod sink;
pub mod source;
pub mod status;

use std::{
    path::PathBuf,
    sync::{atomic::AtomicUsize, Arc},
};

use error::SourceError;
use futures_util::{stream::FuturesUnordered, StreamExt};
use lazysort::SortedBy;
use snops_common::{
    aot_cmds::{AotCmd, Authorization},
    state::{CannonId, EnvId, NetworkId, StorageId},
};
use status::{TransactionStatus, TransactionStatusSender};
use tokio::{
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        Semaphore,
    },
    task::AbortHandle,
};
use tracing::{trace, warn};

use self::{
    error::{CannonError, CannonInstanceError, ExecutionContextError},
    file::TransactionSink,
    sink::TxSink,
    source::TxSource,
};
use crate::{
    cannon::source::{ComputeTarget, QueryTarget},
    state::{GlobalState, REST_CLIENT},
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

/// Transaction cannon state
/// using the `TxSource` and `TxSink` for configuration.
#[derive(Debug)]
pub struct CannonInstance {
    pub id: CannonId,
    // a copy of the global state
    global_state: Arc<GlobalState>,

    pub source: TxSource,
    pub sink: TxSink,

    /// The test_id/storage associated with this cannon.
    /// To point at an external node, create a topology with external node
    /// To generate ahead-of-time, upload a test with a timeline referencing a
    /// cannon pointing at a file
    pub env_id: EnvId,
    pub network: NetworkId,

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
    auth_sender: UnboundedSender<(Authorization, TransactionStatusSender)>,

    pub(crate) fired_txs: Arc<AtomicUsize>,
}

pub struct CannonReceivers {
    transactions: UnboundedReceiver<String>,
    authorizations: UnboundedReceiver<(Authorization, TransactionStatusSender)>,
}

pub type CannonInstanceMeta = (EnvId, NetworkId, StorageId, PathBuf);

impl CannonInstance {
    /// Create a new active transaction cannon
    /// with the given source and sink.
    ///
    /// Locks the global state's tests and storage for reading.
    pub fn new(
        global_state: Arc<GlobalState>,
        id: CannonId,
        (env_id, network, storage_id, aot_bin): CannonInstanceMeta,
        source: TxSource,
        sink: TxSink,
    ) -> Result<(Self, CannonReceivers), CannonError> {
        let (tx_sender, tx_receiver) = tokio::sync::mpsc::unbounded_channel();
        let query_port = source.get_query_port()?;
        let fired_txs = Arc::new(AtomicUsize::new(0));

        let storage_path = global_state.storage_path(network, storage_id);

        // spawn child process for ledger service if the source is local
        let child = query_port
            .map(|port| AotCmd::new(aot_bin, network).ledger_query(storage_path, port))
            .transpose()
            .map_err(|e| CannonError::Command(id, e))?;

        let (auth_sender, auth_receiver) = tokio::sync::mpsc::unbounded_channel();

        Ok((
            Self {
                id,
                global_state,
                source,
                sink,
                env_id,
                network,
                tx_sender,
                auth_sender,
                query_port,
                child,
                task: None,
                fired_txs,
            },
            CannonReceivers {
                transactions: tx_receiver,
                authorizations: auth_receiver,
            },
        ))
    }

    pub fn ctx(&self) -> ExecutionContext {
        ExecutionContext {
            id: self.id,
            env_id: self.env_id,
            network: self.network,
            source: self.source.clone(),
            sink: self.sink.clone(),
            fired_txs: Arc::clone(&self.fired_txs),
            state: Arc::clone(&self.global_state),
        }
    }

    pub fn spawn_local(
        &mut self,
        rx: CannonReceivers,
        env_ready: Arc<Semaphore>,
    ) -> Result<(), CannonError> {
        let ctx = self.ctx();

        let handle = tokio::task::spawn(async move {
            // wait for the cannons to be ready
            let _ = env_ready.acquire().await;

            ctx.spawn(rx).await
        });
        self.task = Some(handle.abort_handle());

        Ok(())
    }

    pub async fn spawn(&mut self, rx: CannonReceivers) -> Result<(), CannonError> {
        self.ctx().spawn(rx).await
    }

    /// Get an expected local query address for this cannon
    pub fn get_local_query(&self) -> String {
        format!(
            "http://{}/api/v1/env/{}/cannons/{}",
            self.global_state.config.get_local_addr(),
            self.env_id,
            self.id
        )
    }

    /// Called by axum to forward /cannon/<id>/<network>/latest/stateRoot
    /// to the ledger query service's /<network>/latest/stateRoot
    pub async fn proxy_state_root(&self) -> Result<String, CannonError> {
        let cannon_id = self.id;
        let env_id = self.env_id;
        let network = self.network;

        match &self.source.query {
            QueryTarget::Local(qs) => {
                if let Some(port) = self.query_port {
                    qs.get_state_root(network, port).await
                } else {
                    Err(CannonInstanceError::MissingQueryPort(cannon_id).into())
                }
            }
            QueryTarget::Node(target) => {
                // shortcut to cached state root if the target is all nodes
                if target.is_all() {
                    if let Some(info) = self.global_state.get_env_block_info(env_id) {
                        return Ok(info.state_root);
                    }
                }

                Ok(self
                    .global_state
                    .snarkos_get::<String>(env_id, "/stateRoot/latest", target)
                    .await?)
            }
        }
    }

    /// Called by axum to forward /cannon/<id>/<network>/transaction/broadcast
    /// to the desired sink
    pub fn proxy_broadcast(&self, body: String) -> Result<(), CannonError> {
        self.tx_sender
            .send(body)
            .map_err(|e| CannonError::SendTxError(self.id, e))?;

        Ok(())
    }

    /// Called by axum to forward /cannon/<id>/auth to a listen source
    pub fn proxy_auth(
        &self,
        body: Authorization,
        events: TransactionStatusSender,
    ) -> Result<(), CannonError> {
        self.auth_sender
            .send((body, events))
            .map_err(|e| CannonError::SendAuthError(self.id, e))?;

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
    network: NetworkId,
    source: TxSource,
    sink: TxSink,
    fired_txs: Arc<AtomicUsize>,
}

impl ExecutionContext {
    pub async fn spawn(self, mut rx: CannonReceivers) -> Result<(), CannonError> {
        let ExecutionContext {
            id: cannon_id,
            env_id,
            source,
            sink,
            fired_txs,
            state,
            ..
        } = &self;

        let env_id = *env_id;
        let env = state
            .get_env(env_id)
            .ok_or_else(|| ExecutionContextError::EnvDropped(env_id, *cannon_id))?;

        trace!("cannon {env_id}.{cannon_id} spawned");

        // get the query path from the realtime tx source
        let suffix = format!("/api/v1/env/{}/cannons/{cannon_id}", env.id);
        let query_path = match source.compute {
            // agents already know the host of the control plane
            ComputeTarget::Agent { .. } => suffix,
            // demox needs to locate it
            ComputeTarget::Demox { .. } => {
                let host = state
                    .config
                    .hostname
                    .as_ref()
                    .ok_or(ExecutionContextError::NoHostnameConfigured)?;
                format!("{host}:{}{suffix}", state.config.port)
            }
        };
        trace!("cannon {env_id}.{cannon_id} using realtime query {query_path}");

        let sink_pipe = match &sink {
            TxSink::Record { file_name, .. } => {
                let pipe = env.sinks.get(file_name).cloned();
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

        let mut auth_execs = FuturesUnordered::new();
        let mut tx_shots = FuturesUnordered::new();

        loop {
            tokio::select! {
                // ------------------------
                // Work generation
                // ------------------------

                // receive authorizations and forward the executions to the compute target
                Some((auth, events)) = rx.authorizations.recv() => {
                    auth_execs.push(self.execute_auth(auth, &query_path, events));
                }
                // receive transactions and forward them to the sink target
                Some(tx) = rx.transactions.recv() => {
                    tx_shots.push(self.fire_tx(sink_pipe.clone(), tx));
                }

                // ------------------------
                // Work results
                // ------------------------

                Some(res) = auth_execs.next() => {
                    if let Err(e) = res {
                        warn!("cannon {env_id}.{cannon_id} auth execute task failed: {e}");
                    }
                },
                Some(res) = tx_shots.next() => {
                    match res {
                        Ok(()) => {
                            let fired_count = fired_txs.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                            trace!("cannon {env_id}.{cannon_id} fired {fired_count} txs");
                        }
                        Err(e) => {
                            warn!("cannon {env_id}.{cannon_id} failed to fire transaction {e}");
                        }
                    }
                },
            }
        }
    }

    /// Execute an authorization on the source's compute target
    async fn execute_auth(
        &self,
        auth: Authorization,
        query_path: &str,
        events: TransactionStatusSender,
    ) -> Result<(), CannonError> {
        let env = self.state.get_env(self.env_id).ok_or_else(|| {
            events.send(TransactionStatus::ExecuteAborted);
            ExecutionContextError::EnvDropped(self.env_id, self.id)
        })?;

        events.send(TransactionStatus::ExecuteQueued);
        match self
            .source
            .compute
            .execute(&self.state, &env, query_path, &auth, &events)
            .await
        {
            // requeue the auth if no agents are available
            Err(CannonError::Source(SourceError::NoAvailableAgents(_))) => {
                warn!(
                    "cannon {}.{} no available agents to execute auth, retrying in a second...",
                    self.env_id, self.id
                );
                events.send(TransactionStatus::ExecuteAwaitingCompute);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                if let Some(cannon) = env.get_cannon(self.id) {
                    cannon.proxy_auth(auth, events)?
                }
                Ok(())
            }
            Err(e) => {
                events.send(TransactionStatus::ExecuteFailed(e.to_string()));
                Err(e)
            }
            res => res,
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
                let cannon_id = self.id;
                let env_id = self.env_id;

                let broadcast_nodes = self.state.get_scored_peers(env_id, target);

                if broadcast_nodes.is_empty() {
                    return Err(ExecutionContextError::NoAvailableAgents(
                        env_id,
                        cannon_id,
                        "to broadcast transactions",
                    )
                    .into());
                }

                let network = self.network;

                // broadcast to the first responding node
                for (_, _, agent, addr) in
                    broadcast_nodes.into_iter().sorted_by(|a, b| a.0.cmp(&b.0))
                {
                    if let Some(id) = agent {
                        // ensure the client is connected
                        let Some(client) = self.state.get_client(id) else {
                            continue;
                        };

                        if let Err(e) = client.broadcast_tx(tx.clone()).await {
                            warn!(
                                "cannon {env_id}.{cannon_id} failed to broadcast transaction to agent {id}: {e}"
                            );
                            continue;
                        }
                        return Ok(());
                    }

                    if let Some(addr) = addr {
                        let url = format!("http://{addr}/{network}/transaction/broadcast");
                        let req = REST_CLIENT
                            .post(url)
                            .header("Content-Type", "application/json")
                            .body(tx.clone())
                            .send();
                        let Ok(res) =
                            tokio::time::timeout(std::time::Duration::from_secs(5), req).await
                        else {
                            warn!("cannon {env_id}.{cannon_id} failed to broadcast transaction to {addr}: timeout");
                            continue;
                        };

                        match res {
                            Err(e) => {
                                warn!(
                                    "cannon {env_id}.{cannon_id} failed to broadcast transaction to {addr}: {e}"
                                );
                                continue;
                            }
                            Ok(req) => {
                                if !req.status().is_success() {
                                    warn!(
                                        "cannon {env_id}.{cannon_id} failed to broadcast transaction to {addr}: {}",
                                        req.status(),
                                    );
                                    continue;
                                }
                            }
                        }

                        return Ok(());
                    }
                }

                Err(ExecutionContextError::NoAvailableAgents(
                    env_id,
                    cannon_id,
                    "to broadcast transactions",
                ))?
            }
        }
        Ok(())
    }
}
