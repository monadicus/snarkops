use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use snops_common::{
    api::AgentEnvInfo,
    binaries::BinaryEntry,
    rpc::error::ReconcileError,
    state::{
        AgentState, HeightRequest, NodeState, ReconcileCondition, ReconcileOptions, TransferId,
    },
};
use tarpc::context;
use tokio::{
    select,
    sync::{Mutex, mpsc::Receiver},
    task::AbortHandle,
    time::sleep_until,
};
use tracing::{error, info, trace};

use super::{
    Reconcile, ReconcileStatus,
    command::NodeCommand,
    process::ProcessContext,
    state::EnvState,
    storage::{BinaryReconciler, GenesisReconciler, LedgerModifyResult, StorageVersionReconciler},
};
use crate::{
    db::Database,
    reconcile::{
        address::AddressResolveReconciler, default_binary, process::EndProcessReconciler,
        storage::LedgerReconciler,
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
    ($id:ident, $e:expr_2021) => {
        reconcile!($id, $e, res => {})
    };
    ($id:ident, $e:expr_2021, $v:ident => $rest:expr_2021) => {
        let $v = $e.reconcile().await?;
        if $v.is_requeue() {
            trace!("Requeue needed for {} ({:?}) {:?}", stringify!($id), $v.scopes, $v.conditions);
            return Ok($v.add_scope(concat!(stringify!($id), "/requeue")));
        }
        $rest
    };
}

impl AgentStateReconciler {
    pub async fn loop_forever(
        &mut self,
        mut reconcile_requests: Receiver<(Instant, ReconcileOptions)>,
    ) {
        let mut err_backoff = 0;

        // The first reconcile is scheduled for 5 seconds after startup.
        // Connecting to the controlplane will likely trigger a reconcile sooner.
        let mut next_reconcile_at = Instant::now() + Duration::from_secs(5);
        let mut next_opts = ReconcileOptions::default();

        // Repeated reconcile loop
        loop {
            // Await for the next reconcile, allowing for it to be moved up sooner
            loop {
                select! {
                    // Replace the next_reconcile_at with the soonest reconcile time
                    Some((new_reconcile_at, opts)) = reconcile_requests.recv() => {
                        next_reconcile_at = next_reconcile_at.min(new_reconcile_at);
                        next_opts = next_opts.union(opts);
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

            // Clear the env info if refetch_info is set to force it to be fetched again
            if next_opts.refetch_info {
                self.state.set_env_info(None).await;
            }

            // If the agent is forced to shutdown, set the shutdown_pending flag
            if next_opts.force_shutdown && self.has_process() {
                self.context.shutdown_pending = true;
            }

            // If the agent is forced to clear the last height, clear it
            if next_opts.clear_last_height {
                self.context.ledger_last_height = None;
                if let Err(e) = self.state.db.set_last_height(None) {
                    error!("failed to clear last height from db: {e}");
                }
            }

            next_opts = Default::default();

            trace!("Reconciling agent state...");
            let res = self.reconcile().await;

            // If this reconcile was triggered by a reconcile request, post the status
            if let Some(client) = self.state.get_ws_client().await {
                let node_is_started = self
                    .state
                    .get_node_status()
                    .await
                    .is_some_and(|s| s.is_started());
                let res = res
                    .clone()
                    .map(|s| s.replace_inner(self.is_node_running() && node_is_started));

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
                    self.state.set_node_status(None).await;
                    self.context.shutdown_pending = false;
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

    pub fn has_process(&self) -> bool {
        self.context.process.is_some()
    }

    pub fn is_node_running(&mut self) -> bool {
        self.context
            .process
            .as_mut()
            .is_some_and(|p| p.is_running())
    }

    pub fn is_shutdown_pending(&self, node: &NodeState, env_info: &AgentEnvInfo) -> bool {
        // Ensure the process is running
        if !self.has_process() {
            return false;
        }

        // Node was already marked for shutdown
        if self.context.shutdown_pending {
            return true;
        }

        // Node is now configured to be offline
        if !node.online {
            info!("Node is marked offline");
            return true;
        }

        // Check if the storage version, storage id, or network id has changed
        if self
            .context
            .env_state
            .as_ref()
            .is_none_or(|e| e.changed(env_info))
        {
            info!("Node storage version, storage id, or network id has changed");
            return true;
        }

        // Check if the ledger height is not resolved
        if self.context.ledger_last_height != Some(node.height) && !node.height.1.is_top() {
            info!("Node ledger target height has changed");
            return true;
        }

        let default_binary = default_binary(env_info);
        let target_binary = env_info
            .storage
            .binaries
            .get(&node.binary.unwrap_or_default())
            .unwrap_or(&default_binary);

        // Check if the binary this node is running is different from the one in storage
        if self.context.process.as_ref().is_some_and(|p| {
            target_binary
                .sha256
                .as_ref()
                .is_some_and(|sha256| !p.is_sha256_eq(sha256))
        }) {
            info!("Node binary for the running process has changed");
            return true;
        }

        // Check if the binary this node is running is different from the one in storage
        if self
            .context
            .transfers
            .as_ref()
            .and_then(|t| t.binary_transfer.as_ref())
            .is_some_and(|(_, bin)| bin != target_binary)
        {
            info!("Node binary has changed");
            return true;
        }

        false
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

        // If the node should be torn down because a configuration changed, we need to
        // gracefully shut down the node.
        if self.is_shutdown_pending(node, &env_info) {
            self.context.shutdown_pending = true;
            // Unwrap safety - is_shutdown_pending ensures the process exists.
            let process = self.context.process.as_mut().unwrap();

            reconcile!(end_process, EndProcessReconciler(process), res => {
                // If the process has exited, clear the process context
                if res.inner.is_some() {
                    self.context.process = None;
                    self.state.set_node_status(None).await;
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

                return Ok(ReconcileStatus::empty()
                    .requeue_after(Duration::ZERO)
                    .add_scope("agent_state/exited"));
            }

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
                let Some(node_status) = self.state.get_node_status().await else {
                    return Ok(ReconcileStatus::empty().add_scope("agent_state/node/booting"));
                };

                let rec = if node_status.is_started() {
                    ReconcileStatus::default()
                } else if node_status.is_stopped() {
                    // Terminate looping after some kind of failure
                    ReconcileStatus::empty()
                } else {
                    ReconcileStatus::empty().requeue_after(Duration::from_secs(5))
                };

                return Ok(rec.add_scope(format!("agent_state/node/{}", node_status.label())));
            }

            // If the node is not online, the process is still running, but the node
            // has not connected to the controlplane.
            // This can happen if the node is still syncing, or if the controlplane
            // is not reachable.
            return Ok(ReconcileStatus::empty()
                .requeue_after(Duration::from_secs(1))
                .add_condition(ReconcileCondition::PendingStartup)
                .add_scope("agent_state/node/booting"));
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

        info!("Starting node process");
        let command = NodeCommand::new(
            Arc::clone(&self.state),
            node_arc,
            *env_id,
            Arc::clone(&env_info),
        )
        .await?;

        let process = ProcessContext::new(command)?;
        // Clear the last node running status (it was shut down)
        self.state.set_node_status(None).await;
        self.context.process = Some(process);
        self.context.shutdown_pending = false;
        Ok(ReconcileStatus::empty()
            .add_scope("agent_state/node/booting")
            .requeue_after(Duration::from_secs(1)))
    }
}

// TODO: large file download behavior (ledgers):
// same as above, except maybe chunk the downloads or

// TODO: support ledger.aleo.network snapshots:
// https://ledger.aleo.network/mainnet/snapshot/latest.txt
// https://ledger.aleo.network/testnet/snapshot/latest.txt
// https://ledger.aleo.network/canarynet/snapshot/latest.txt
