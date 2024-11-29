use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use snops_common::{
    binaries::BinaryEntry,
    rpc::error::ReconcileError,
    state::{AgentState, HeightRequest, ReconcileCondition, TransferId},
};
use tarpc::context;
use tokio::{
    select,
    sync::{mpsc::Receiver, Mutex},
    task::AbortHandle,
    time::sleep_until,
};
use tracing::{error, info, trace};

use super::{
    command::NodeCommand,
    process::ProcessContext,
    state::EnvState,
    storage::{BinaryReconciler, GenesisReconciler, LedgerModifyResult, StorageVersionReconciler},
    Reconcile, ReconcileStatus,
};
use crate::{
    db::Database,
    reconcile::{
        address::AddressResolveReconciler, process::EndProcessReconciler, storage::LedgerReconciler,
    },
    state::GlobalState,
};

/// Attempt to reconcile the agent's current state.
/// This will download files and start/stop the node
pub struct AgentStateReconciler {
    pub agent_state: Arc<AgentState>,
    pub state: Arc<GlobalState>,
    pub context: AgentStateReconcilerContext,
}

#[derive(Default)]
pub struct AgentStateReconcilerContext {
    /// Persisted values that determine if the storage has changed
    pub env_state: Option<EnvState>,
    /// The last ledger height that was successfully configured
    pub ledger_last_height: Option<(usize, HeightRequest)>,
    // TODO: allow transfers to be interrupted. potentially allow them to be resumed by using the
    // file range feature.
    /// Information about active transfers
    transfers: Option<TransfersContext>,
    /// Information about the node process
    pub process: Option<ProcessContext>,
    pub shutdown_pending: bool,
}

#[derive(Default)]
struct TransfersContext {
    /// Metadata about an active binary transfer
    binary_transfer: Option<(TransferId, BinaryEntry)>,
    /// Time the binary was marked as OK
    binary_ok_at: Option<Instant>,

    /// Metadata about an active genesis block transfer
    genesis_transfer: Option<TransferId>,
    /// Time the genesis block was marked as OK
    genesis_ok_at: Option<Instant>,

    /// The height that is currently being configured
    ledger_pending_height: Option<(usize, HeightRequest)>,

    /// A handle containing the task that modifies the ledger.
    /// The mutex is held until the task is complete, and the bool is set to
    /// true when the task is successful.
    ledger_modify_handle: Option<(AbortHandle, Arc<Mutex<Option<LedgerModifyResult>>>)>,
}

impl AgentStateReconcilerContext {
    pub fn hydrate(db: &Database) -> Self {
        let ledger_last_height = db
            .last_height()
            .inspect_err(|e| error!("failed to restore last height from db: {e}"))
            .unwrap_or_default();
        let env_state = db
            .env_state()
            .inspect_err(|e| error!("failed to restore env state from db: {e}"))
            .unwrap_or_default();

        Self {
            env_state,
            ledger_last_height,
            ..Default::default()
        }
    }
}

/// Run a reconciler and return early if a requeue is needed. A condition is
/// added to the scope when a requeue is needed to provide more context when
/// monitoring the agent.
macro_rules! reconcile {
    ($id:ident, $e:expr) => {
        reconcile!($id, $e, res => {})
    };
    ($id:ident, $e:expr, $v:ident => $rest:expr) => {
        let $v = $e.reconcile().await?;
        if $v.is_requeue() {
            trace!("Requeue needed for {} ({:?}) {:?}", stringify!($id), $v.scopes, $v.conditions);
            return Ok($v.add_scope(concat!(stringify!($id), "/requeue")));
        }
        $rest
    };
}

impl AgentStateReconciler {
    pub async fn loop_forever(&mut self, mut reconcile_requests: Receiver<Instant>) {
        let mut err_backoff = 0;

        // The first reconcile is scheduled for 5 seconds after startup.
        // Connecting to the controlplane will likely trigger a reconcile sooner.
        let mut next_reconcile_at = Instant::now() + Duration::from_secs(5);

        // Repeated reconcile loop
        loop {
            // Await for the next reconcile, allowing for it to be moved up sooner
            loop {
                select! {
                    // Replace the next_reconcile_at with the soonest reconcile time
                    Some(new_reconcile_at) = reconcile_requests.recv() => {
                        next_reconcile_at = next_reconcile_at.min(new_reconcile_at);
                    },
                    _ = sleep_until(next_reconcile_at.into()) => {
                        break
                    }
                }
            }

            // Drain the reconcile request queue
            while reconcile_requests.try_recv().is_ok() {}
            // Schedule the next reconcile for 1 minute (to periodically check if the node
            // went offline)
            next_reconcile_at = Instant::now() + Duration::from_secs(60);

            // Update the reconciler with the latest agent state
            // This prevents the agent state from changing during reconciliation
            self.agent_state = self.state.get_agent_state().await;

            trace!("Reconciling agent state...");
            let res = self.reconcile().await;
            if let Some(client) = self.state.get_ws_client().await {
                let res = res.clone();
                // TODO: throttle this broadcast
                tokio::spawn(async move {
                    if let Err(e) = client.post_reconcile_status(context::current(), res).await {
                        error!("failed to post reconcile status: {e}");
                    }
                });
            }
            match res {
                Ok(status) => {
                    if status.inner.is_some() {
                        err_backoff = 0;
                        trace!("Reconcile completed");
                    }
                    if !status.conditions.is_empty() {
                        trace!("Reconcile conditions: {:?}", status.conditions);
                    }
                    if let Some(requeue_after) = status.requeue_after {
                        trace!("Requeueing after {requeue_after:?}");
                        next_reconcile_at = Instant::now() + requeue_after;
                    }
                }
                Err(e) => {
                    error!("failed to reconcile agent state: {e}");
                    err_backoff = (err_backoff + 5).min(30);
                    next_reconcile_at = Instant::now() + Duration::from_secs(err_backoff);
                }
            }
        }
    }

    pub async fn reconcile_inventory(&mut self) -> Result<ReconcileStatus<()>, ReconcileError> {
        // TODO: cleanup other things

        // End the process if it is running
        if let Some(process) = self.context.process.as_mut() {
            reconcile!(end_process, EndProcessReconciler(process), res => {
                // If the process has exited, clear the process context
                if res.inner.is_some() {
                    self.context.process = None;
                }
            });
        }

        if let Some(_transfers) = self.context.transfers.as_mut() {
            // Clear the env state
            self.context.env_state = None;
            if let Err(e) = self.state.db.set_env_state(None) {
                error!("failed to clear env state from db: {e}");
            }
            // Clear the last height
            self.context.ledger_last_height = None;
            if let Err(e) = self.state.db.set_last_height(None) {
                error!("failed to clear last height from db: {e}");
            }

            // TODO: interrupt/kill off pending downloads

            // Destroy the old transfers context
            self.context.transfers = None;
        }

        Ok(ReconcileStatus::default().add_scope("agent_state/inventory"))
    }
}

impl Reconcile<(), ReconcileError> for AgentStateReconciler {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError> {
        let (env_id, node) = match self.agent_state.as_ref() {
            AgentState::Inventory => {
                return self.reconcile_inventory().await;
            }
            AgentState::Node(env_id, node) => (env_id, node),
        };

        let env_info = self.state.get_env_info(*env_id).await?;

        // Check if the storage version, storage id, or network id has changed
        let storage_has_changed = self
            .context
            .env_state
            .as_ref()
            .map(|e| e.changed(&env_info))
            .unwrap_or(true);

        // Check if the ledger height is not resolved
        let height_has_changed =
            self.context.ledger_last_height != Some(node.height) && !node.height.1.is_top();

        // If the node should be torn down, or the storage has changed, we need to
        // gracefully shut down the node.
        let shutdown_pending = !node.online || storage_has_changed || height_has_changed;

        if let (true, Some(process)) = (
            shutdown_pending || self.context.shutdown_pending,
            self.context.process.as_mut(),
        ) {
            self.context.shutdown_pending = true;
            reconcile!(end_process, EndProcessReconciler(process), res => {
                // If the process has exited, clear the process context
                if res.inner.is_some() {
                    self.context.process = None;
                    self.context.shutdown_pending = false;
                }
            });
        }

        // node is offline, no need to reconcile
        if !node.online {
            return Ok(ReconcileStatus::default().add_scope("agent_state/offline"));
        }

        let node_arc = Arc::new(*node.clone());

        // Resolve the addresses of the peers and validators
        // This is run before the process is started, as the agent can sometimes have
        // new addresses that need to be resolved.
        reconcile!(
            address_resolve,
            AddressResolveReconciler {
                node: Arc::clone(&node_arc),
                state: Arc::clone(&self.state),
            }
        );

        // Reconcile behavior while the node is running...
        if let Some(process) = self.context.process.as_mut() {
            // If the process has exited, clear the process context
            if !process.is_running() {
                info!("Node process has exited...");
                self.context.process = None;
            } else {
                // Accumulate all the fields that are used to derive the command that starts
                // the node.
                let command = NodeCommand::new(
                    Arc::clone(&self.state),
                    node_arc,
                    *env_id,
                    Arc::clone(&env_info),
                )
                .await?;

                // If the command has changed, restart the process
                if process.command != command {
                    info!("Node command has changed, restarting process...");
                    self.context.shutdown_pending = true;
                    return Ok(ReconcileStatus::empty()
                        .add_scope("agent_state/command_changed")
                        .requeue_after(Duration::ZERO));
                }

                // Prevent other reconcilers from running while the node is running
                if self.state.is_node_online() {
                    return Ok(ReconcileStatus::default().add_scope("agent_state/running"));
                } else {
                    // If the node is not online, the process is still running, but the node
                    // has not connected to the controlplane.
                    // This can happen if the node is still syncing, or if the controlplane
                    // is not reachable.
                    return Ok(ReconcileStatus::empty()
                        .requeue_after(Duration::from_secs(1))
                        .add_condition(ReconcileCondition::PendingStartup)
                        .add_scope("agent_state/starting"));
                }
            }
        }

        let storage_path = self
            .state
            .cli
            .storage_path(env_info.network, env_info.storage.id);

        // Ensure the storage version is correct, deleting the storage path
        // the version changes.
        reconcile!(
            storage_version,
            StorageVersionReconciler(&storage_path, env_info.storage.version),
            res => {
                if res.inner.is_some() {
                    trace!("Transfers context cleared due to storage version change");
                    self.context.transfers = None;
                }
            }
        );

        // Initialize the transfers context with the current status
        // This happens after the StorageVersionReconciler as storage_version within
        // env_state will be guaranteed to match the remote env after it succeeds.
        if self.context.transfers.is_none() {
            let env_state = EnvState::from(env_info.as_ref());
            if let Err(e) = self.state.db.set_env_state(Some(&env_state)) {
                error!("failed to save env state to db: {e}");
            }
            self.context.env_state = Some(env_state);
            self.context.transfers = Some(Default::default());
            trace!("Cleared transfers state...");
        }
        let transfers = self.context.transfers.as_mut().unwrap();

        // Resolve the genesis block
        reconcile!(
            genesis,
            GenesisReconciler {
                state: Arc::clone(&self.state),
                env_info: Arc::clone(&env_info),
                transfer: &mut transfers.genesis_transfer,
                ok_at: &mut transfers.genesis_ok_at,
            }
        );

        // Resolve the node's binary
        reconcile!(
            binary,
            BinaryReconciler {
                state: Arc::clone(&self.state),
                env_info: Arc::clone(&env_info),
                node_binary: node.binary,
                transfer: &mut transfers.binary_transfer,
                ok_at: &mut transfers.binary_ok_at,
            }
        );

        reconcile!(
            ledger,
            LedgerReconciler {
                state: Arc::clone(&self.state),
                env_info: Arc::clone(&env_info),
                modify_handle: &mut transfers.ledger_modify_handle,
                target_height: node.height,
                last_height: &mut self.context.ledger_last_height,
                pending_height: &mut transfers.ledger_pending_height,
            }
        );

        // TODO: if possible, use the NodeCommand as configuration for a node service to
        // allow running the node outside of the agent

        if self.context.process.is_none() {
            info!("Starting node process");
            let command = NodeCommand::new(
                Arc::clone(&self.state),
                node_arc,
                *env_id,
                Arc::clone(&env_info),
            )
            .await?;

            let process = ProcessContext::new(command)?;
            self.context.process = Some(process);
            return Ok(ReconcileStatus::empty()
                .add_scope("agent_state/starting")
                .requeue_after(Duration::from_secs(1)));
        }

        Ok(ReconcileStatus::empty()
            .add_scope("agent_state/edge_case")
            .requeue_after(Duration::from_secs(1)))
    }
}

// TODO: large file download behavior (ledgers):
// same as above, except maybe chunk the downloads or

// TODO: support ledger.aleo.network snapshots:
// https://ledger.aleo.network/mainnet/snapshot/latest.txt
// https://ledger.aleo.network/testnet/snapshot/latest.txt
// https://ledger.aleo.network/canarynet/snapshot/latest.txt
