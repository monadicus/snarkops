pub mod context;
pub mod error;
pub mod file;
mod net;
pub mod router;
pub mod sink;
pub mod source;
pub mod tracker;

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize},
        Arc,
    },
};

use context::ExecutionContext;
use dashmap::DashMap;
use snops_common::{
    aot_cmds::AotCmd,
    format::PackedUint,
    state::{Authorization, CannonId, EnvId, NetworkId, StorageId, TransactionSendState},
};
use tokio::{
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        Semaphore,
    },
    task::AbortHandle,
};
use tracing::{error, trace, warn};
use tracker::TransactionTracker;

use self::{
    error::{CannonError, CannonInstanceError},
    sink::TxSink,
    source::TxSource,
};
use crate::{cannon::source::QueryTarget, state::GlobalState};

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

    /// channel to send transaction ids to the the task
    pub(crate) tx_sender: UnboundedSender<Arc<String>>,
    /// channel to send authorizations (by transaction id) to the the task
    pub(crate) auth_sender: UnboundedSender<Arc<String>>,
    /// transaction ids that are currently being processed
    pub(crate) transactions: Arc<DashMap<Arc<String>, TransactionTracker>>,

    pub(crate) received_txs: Arc<AtomicU64>,
    pub(crate) fired_txs: Arc<AtomicUsize>,
}

pub struct CannonReceivers {
    transactions: UnboundedReceiver<Arc<String>>,
    authorizations: UnboundedReceiver<Arc<String>>,
}

pub type CannonInstanceMeta = (EnvId, NetworkId, StorageId, PathBuf);

impl CannonInstance {
    /// Increment and save the received transaction count
    pub(crate) fn inc_received_txs(
        state: &GlobalState,
        env_id: EnvId,
        cannon_id: CannonId,
        txs: &AtomicU64,
    ) -> u64 {
        let index = txs.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Err(e) = state.db.tx_index.save(
            &(env_id, cannon_id, Arc::new(String::new())),
            &PackedUint(index),
        ) {
            error!("cannon {env_id}.{cannon_id} failed to save received tx count: {e}");
        }
        index
    }

    /// Load transactions for this cannon/env from the store
    fn restore_transactions(
        state: &GlobalState,
        env_id: EnvId,
        cannon_id: CannonId,
    ) -> (DashMap<Arc<String>, TransactionTracker>, AtomicU64) {
        let transactions = DashMap::new();

        // Restore the received transaction count (empty string key for tx_index)
        let received_txs =
            match state
                .db
                .tx_index
                .restore(&(env_id, cannon_id, Arc::new(String::new())))
            {
                Ok(Some(index)) => AtomicU64::new(index.0),
                Ok(None) => AtomicU64::new(0),
                Err(e) => {
                    error!("cannon {env_id}.{cannon_id} failed to parse received tx count: {e}");
                    AtomicU64::new(0)
                }
            };

        let statuses = match state.db.tx_status.read_with_prefix(&(env_id, cannon_id)) {
            Ok(statuses) => statuses,
            Err(e) => {
                error!("cannon {env_id}.{cannon_id} failed to restore transaction statuses: {e}");
                return (transactions, received_txs);
            }
        };

        // Walk through the statuses and restore the transactions (every transaction has
        // a status)
        for (key, status) in statuses {
            // Ensure the transaction has an index
            let index = match state.db.tx_index.restore(&key) {
                Ok(Some(index)) => index.0,
                Ok(None) => {
                    warn!(
                        "cannon {env_id}.{cannon_id} failed to restore index for transaction {} (missing index)", key.2
                    );
                    continue;
                }
                Err(e) => {
                    error!(
                        "cannon {env_id}.{cannon_id} failed to parse index for transaction {}: {e}",
                        key.2
                    );
                    continue;
                }
            };

            let authorization = match state.db.tx_auths.restore(&key) {
                Ok(auth) => auth.map(Arc::new),
                Err(e) => {
                    error!(
                        "cannon {env_id}.{cannon_id} failed to restore authorization for transaction {}: {e}",
                        key.2
                    );
                    continue;
                }
            };

            // Restore the transaction, if it exists. If there is an issue restoring the
            // transaction, it is possible to re-execute the authorization when
            // present.
            let transaction = match state.db.tx_blobs.restore(&key) {
                Ok(tx) => tx.map(Arc::new),
                Err(e) => {
                    if authorization.is_some() {
                        warn!(
                            "cannon {env_id}.{cannon_id} failed to restore json for transaction {}: {e}. Recovering from authorization",
                            key.2
                        );
                        None
                    } else {
                        error!(
                            "cannon {env_id}.{cannon_id} failed to restore json for transaction {}: {e}",
                            key.2
                        );
                        continue;
                    }
                }
            };

            transactions.insert(
                key.2,
                TransactionTracker {
                    index,
                    authorization,
                    transaction,
                    status,
                },
            );
        }

        (transactions, received_txs)
    }

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
        let (transactions, received_txs) = Self::restore_transactions(&global_state, env_id, id);

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
                received_txs: Arc::new(received_txs),
                transactions: Arc::new(transactions),
            },
            CannonReceivers {
                transactions: tx_receiver,
                authorizations: auth_receiver,
            },
        ))
    }

    /// Create an execution context for this cannon
    pub fn ctx(&self) -> ExecutionContext {
        ExecutionContext {
            id: self.id,
            env_id: self.env_id,
            network: self.network,
            source: self.source.clone(),
            sink: self.sink.clone(),
            fired_txs: Arc::clone(&self.fired_txs),
            state: Arc::clone(&self.global_state),
            transactions: Arc::clone(&self.transactions),
        }
    }

    /// Spawn the cannon's execution context as an abortable local task
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

    /// Spawn the cannon's execution context and wait for it to finish
    #[deprecated = "originally used in the timeline API for temporary cannons with finite transaction counts"]
    pub async fn spawn(&mut self, rx: CannonReceivers) -> Result<(), CannonError> {
        self.ctx().spawn(rx).await
    }

    /// Get an expected local query address for this cannon
    pub fn get_local_query(&self) -> String {
        format!(
            "http://{}/api/v1/env/{}/cannons/{}",
            self.global_state.cli.get_local_addr(),
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
    pub fn proxy_broadcast(
        &self,
        tx_id: Arc<String>,
        body: serde_json::Value,
    ) -> Result<(), CannonError> {
        let key = (self.env_id, self.id, Arc::clone(&tx_id));

        // if the transaction is in the cache, it has already been broadcasted
        if let Some(cache) = self.global_state.env_network_cache.get(&self.env_id) {
            if cache.has_transaction(&tx_id) {
                if let Err(e) = TransactionTracker::delete(&self.global_state, &key) {
                    error!(
                        "cannon {}.{} failed to delete {tx_id} (in proxy_broadcast): {e:?}",
                        self.env_id, self.id
                    );
                }
                return Err(CannonError::TransactionAlreadyExists(
                    self.id,
                    tx_id.to_string(),
                ));
            }
        }

        // prevent already queued transactions from being re-broadcasted
        let tracker = if let Some(mut tx) = self.transactions.get(&tx_id).as_deref().cloned() {
            // if we receive a transaction that is not executing, it is a duplicate
            if !matches!(tx.status, TransactionSendState::Executing(_)) {
                return Err(CannonError::TransactionAlreadyExists(
                    self.id,
                    tx_id.to_string(),
                ));
            }

            // clear attempts (as this was a successful execute)
            if let Err(e) = TransactionTracker::clear_attempts(&self.global_state, &key) {
                error!(
                    "cannon {}.{} failed to clear attempts for {tx_id} (in proxy_broadcast): {e:?}",
                    self.env_id, self.id
                );
            }
            // update the status to pending broadcast, and write the transaction
            tx.status = TransactionSendState::Unsent;
            tx.transaction = Some(Arc::new(body));
            tx
        } else {
            trace!(
                "cannon {}.{} received broadcast {tx_id}",
                self.env_id,
                self.id
            );
            TransactionTracker {
                index: Self::inc_received_txs(
                    &self.global_state,
                    self.env_id,
                    self.id,
                    &self.received_txs,
                ),
                authorization: None,
                transaction: Some(Arc::new(body)),
                status: TransactionSendState::Unsent,
            }
        };

        // write the transaction to the store to prevent data loss
        tracker.write(&self.global_state, &key)?;
        self.transactions.insert(tx_id.to_owned(), tracker);

        // forward the transaction to the task, which will broadcast it
        // rather than waiting for the next broadcast check cycle
        self.tx_sender
            .send(tx_id)
            .map_err(|e| CannonError::SendTxError(self.id, e))?;

        Ok(())
    }

    /// Called by axum to forward /cannon/<id>/auth to a listen source
    pub async fn proxy_auth(&self, body: Authorization) -> Result<Arc<String>, CannonError> {
        let Some(storage) = self
            .global_state
            .get_env(self.env_id)
            .map(|e| Arc::clone(&e.storage))
        else {
            // this error is very unlikely
            return Err(CannonError::BinaryError(
                self.id,
                "missing environment".to_owned(),
            ));
        };

        // resolve the binary for the compute target
        let compute_bin = storage
            .resolve_compute_binary(&self.global_state)
            .await
            .map_err(|e| CannonError::BinaryError(self.id, e.to_string()))?;
        let aot = AotCmd::new(compute_bin, self.network);

        // derive the transaction id from the authorization
        let tx_id = aot
            .get_tx_id(&body)
            .await
            .map_err(|e| CannonError::BinaryError(self.id, format!("derive tx id: {e}")))?;

        // prevent already queued transactions from being re-computed
        if self.transactions.contains_key(&tx_id) {
            return Err(CannonError::TransactionAlreadyExists(self.id, tx_id));
        }

        let tracker = TransactionTracker {
            index: Self::inc_received_txs(
                &self.global_state,
                self.env_id,
                self.id,
                &self.received_txs,
            ),
            authorization: Some(Arc::new(body)),
            transaction: None,
            status: TransactionSendState::Authorized,
        };

        let tx_id = Arc::new(tx_id);

        // write the transaction to the store to prevent data loss
        tracker.write(
            &self.global_state,
            &(self.env_id, self.id, Arc::clone(&tx_id)),
        )?;
        self.transactions.insert(Arc::clone(&tx_id), tracker);

        trace!("cannon {}.{} received auth {tx_id}", self.env_id, self.id);
        self.auth_sender
            .send(Arc::clone(&tx_id))
            .map_err(|e| CannonError::SendAuthError(self.id, e))?;

        Ok(tx_id)
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
