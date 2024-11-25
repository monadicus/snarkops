use std::{
    collections::HashSet,
    ops::Deref,
    sync::Arc,
    time::{Duration, Instant},
};

use snops_common::{
    api::EnvInfo,
    binaries::BinaryEntry,
    format::{DataFormat, DataHeaderOf},
    rpc::error::ReconcileError2,
    state::{
        AgentId, AgentPeer, AgentState, HeightRequest, NetworkId, NodeState, StorageId, TransferId,
    },
};
use tarpc::context;
use tokio::{
    select,
    sync::{mpsc::Receiver, Mutex},
    task::AbortHandle,
};
use tracing::{error, info, trace, warn};

use super::{
    command::NodeCommand,
    process::ProcessContext,
    storage::{BinaryReconciler, GenesisReconciler, LedgerModifyResult, StorageVersionReconciler},
    DirectoryReconciler, Reconcile, ReconcileStatus,
};
use crate::{
    db::Database,
    reconcile::{process::EndProcessReconciler, storage::LedgerReconciler},
    state::GlobalState,
};

/// Attempt to reconcile the agent's current state.
/// This will download files and start/stop the node
pub struct AgentStateReconciler {
    pub agent_state: Arc<AgentState>,
    pub state: Arc<GlobalState>,
    pub context: AgentStateReconcilerContext,
}

pub struct EnvState {
    network_id: NetworkId,
    storage_id: StorageId,
    storage_version: u16,
}

impl From<&EnvInfo> for EnvState {
    fn from(info: &EnvInfo) -> Self {
        Self {
            network_id: info.network,
            storage_id: info.storage.id,
            storage_version: info.storage.version,
        }
    }
}

impl Default for EnvState {
    fn default() -> Self {
        Self {
            network_id: NetworkId::Mainnet,
            storage_id: StorageId::default(),
            storage_version: 0,
        }
    }
}

#[derive(Default)]
struct TransfersContext {
    /// Persisted values that determine if the storage has changed
    env_state: EnvState,

    /// The last ledger height that was successfully configured
    ledger_last_height: Option<(usize, HeightRequest)>,

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

#[derive(Default)]
pub struct AgentStateReconcilerContext {
    // TODO: allow transfers to be interrupted. potentially allow them to be resumed by using the
    // file range feature.
    /// Information about active transfers
    transfers: Option<TransfersContext>,
    /// Information about the node process
    pub process: Option<ProcessContext>,
    pub shutdown_pending: bool,
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
            transfers: (ledger_last_height.is_some() || env_state.is_some()).then(|| {
                TransfersContext {
                    env_state: env_state.unwrap_or_default(),
                    ledger_last_height,
                    ..Default::default()
                }
            }),
            ..Default::default()
        }
    }
}

impl AgentStateReconciler {
    pub async fn loop_forever(&mut self, mut reconcile_requests: Receiver<Instant>) {
        let mut err_backoff = 0;

        // The first reconcile is scheduled for 5 seconds after startup.
        // Connecting to the controlplane will likely trigger a reconcile sooner.
        let mut next_reconcile_at = Instant::now() + Duration::from_secs(5);
        let mut wait = Box::pin(tokio::time::sleep_until(next_reconcile_at.into()));

        loop {
            // Await for the next reconcile, allowing for it to be moved up sooner
            select! {
                // Replace the next_reconcile_at with the soonest reconcile time
                Some(new_reconcile_at) = reconcile_requests.recv() => {
                    next_reconcile_at = next_reconcile_at.min(new_reconcile_at);
                    wait = Box::pin(tokio::time::sleep_until(next_reconcile_at.into()));
                },
                _ = &mut wait => {}
            }

            // Drain the reconcile request queue
            while reconcile_requests.try_recv().is_ok() {}
            // Schedule the next reconcile for 1 minute (to periodically check if the node
            // went offline)
            next_reconcile_at = Instant::now() + Duration::from_secs(60);

            // Update the reconciler with the latest agent state
            // This prevents the agent state from changing during reconciliation
            self.agent_state = self.state.agent_state.read().await.deref().clone();

            trace!("reconciling agent state...");
            match self.reconcile().await {
                Ok(status) => {
                    if status.inner.is_some() {
                        trace!("reconcile completed");
                    }
                    if !status.conditions.is_empty() {
                        trace!("reconcile conditions: {:?}", status.conditions);
                    }
                    if let Some(requeue_after) = status.requeue_after {
                        next_reconcile_at = Instant::now() + requeue_after;
                    }
                }
                Err(e) => {
                    error!("failed to reconcile agent state: {e}");
                    err_backoff = (err_backoff + 5).min(30);
                    next_reconcile_at = Instant::now() + Duration::from_secs(err_backoff);
                }
            }

            // TODO: announce reconcile status to the server, throttled
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
            return Ok($v.add_scope(concat!(stringify!($id), "/requeue")));
        }
        $rest
    };
}

impl Reconcile<(), ReconcileError2> for AgentStateReconciler {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError2> {
        match self.agent_state.as_ref() {
            AgentState::Inventory => {
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
                    if let Err(e) = self.state.db.set_env_state(None) {
                        error!("failed to clear env state from db: {e}");
                    }
                    if let Err(e) = self.state.db.set_last_height(None) {
                        error!("failed to clear last height from db: {e}");
                    }

                    // TODO: interrupt/kill off pending downloads

                    // Destroy the old transfers context
                    self.context.transfers = None;
                }

                return Ok(ReconcileStatus::default().add_scope("agent_state/inventory"));
            }
            AgentState::Node(env_id, node) => {
                let env_info = self.state.get_env_info(*env_id).await?;

                // Check if the storage version, storage id, or network id has changed
                let storage_has_changed = self
                    .context
                    .transfers
                    .as_ref()
                    .map(|t| t.env_state.changed(&env_info))
                    .unwrap_or(true);

                // If the node should be torn down, or the storage has changed, we need to
                // gracefully shut down the node.
                let shutdown_pending = !node.online || storage_has_changed;

                // TODO: check if addrs have changed, then update the command

                if let (true, Some(process)) = (
                    shutdown_pending || self.context.shutdown_pending,
                    self.context.process.as_mut(),
                ) {
                    self.context.shutdown_pending = true;
                    reconcile!(end_process, EndProcessReconciler(process), res => {
                        // If the process has exited, clear the process context
                        if res.inner.is_some() {
                            self.context.process = None;
                        }
                    });
                }

                // node is offline, no need to reconcile
                if !node.online {
                    return Ok(ReconcileStatus::default().add_scope("agent_state/offline"));
                }

                // Reconcile behavior while the node is running...
                if let Some(process) = self.context.process.as_ref() {
                    // If the process has exited, clear the process context
                    if !process.is_running() {
                        info!("node process has exited...");
                        self.context.process = None;
                    } else {
                        // Prevent other reconcilers from running while the node is running
                        return Ok(ReconcileStatus::default().add_scope("agent_state/running"));
                    }
                }

                let node_arc = Arc::new(*node.clone());

                // Initialize the transfers context with the current status
                if self.context.transfers.is_none() {
                    // TODO: write this to the db
                    let env_state = EnvState::from(env_info.as_ref());
                    if let Err(e) = self.state.db.set_env_state(Some(&env_state)) {
                        error!("failed to save env state to db: {e}");
                    }
                    self.context.transfers = Some(TransfersContext {
                        env_state,
                        ..Default::default()
                    });
                }
                let transfers = self.context.transfers.as_mut().unwrap();

                let storage_path = self
                    .state
                    .cli
                    .storage_path(env_info.network, env_info.storage.id);

                // Ensure the storage version is correct, deleting the storage path
                // the version changes.
                reconcile!(
                    storage,
                    StorageVersionReconciler(&storage_path, env_info.storage.version)
                );

                // Create the storage path if it does not exist
                reconcile!(dir, DirectoryReconciler(&storage_path));

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
                        last_height: &mut transfers.ledger_last_height,
                        pending_height: &mut transfers.ledger_pending_height,
                    }
                );

                // Resolve the addresses of the peers and validators
                // TODO: Set an expiry for resolved addresses
                reconcile!(
                    address_resolve,
                    AddressResolveReconciler {
                        node: Arc::clone(&node_arc),
                        state: Arc::clone(&self.state),
                    }
                );

                // Accumulate all the fields that are used to derive the command that starts
                // the node.
                // This will be used to determine if the command has changed at all.
                let command = NodeCommand::new(
                    Arc::clone(&self.state),
                    node_arc,
                    *env_id,
                    Arc::clone(&env_info),
                )
                .await?;

                if self.context.process.as_ref().map(|p| &p.command) != Some(&command) {
                    // TODO: OK to restart the node -- command has changed
                }

                // TODO: spawn the command, manage its state, check that it's up
                // TODO: if possible, use the NodeCommand as configuration for a node service to
                // allow running the node outside of the agent

                if self.context.process.is_none() {
                    info!("Starting node process");
                    let process = ProcessContext::new(command)?;
                    self.context.process = Some(process);
                }
            }
        }

        Ok(ReconcileStatus::empty())
    }
}

/// Given a node state, resolve the addresses of the agent based peers and
/// validators. Non-agent based peers have their addresses within the state
/// already.
struct AddressResolveReconciler {
    state: Arc<GlobalState>,
    node: Arc<NodeState>,
}

impl Reconcile<(), ReconcileError2> for AddressResolveReconciler {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError2> {
        let AddressResolveReconciler { state, node } = self;

        // Find agents that do not have cached addresses
        let unresolved_addrs: HashSet<AgentId> = {
            let resolved_addrs = state.resolved_addrs.read().await;
            node.peers
                .iter()
                .chain(node.validators.iter())
                .filter_map(|p| {
                    if let AgentPeer::Internal(id, _) = p {
                        (!resolved_addrs.contains_key(id)).then_some(*id)
                    } else {
                        None
                    }
                })
                .collect()
        };

        // All addrs have been resolved.
        // TODO: May need to mark some of these as stale at some point.
        if unresolved_addrs.is_empty() {
            return Ok(ReconcileStatus::default());
        }

        let Some(client) = state.client.read().await.clone() else {
            warn!("Agent state contains {} addresses that need to be resolved, but client is not connected", unresolved_addrs.len());

            // Client is offline so new addrs cannot be requested
            return Ok(ReconcileStatus::default());
        };

        // Fetch all unresolved addresses and update the cache
        tracing::debug!(
            "need to resolve addrs: {}",
            unresolved_addrs
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        // Resolve the addresses
        // TODO: turn this into a background process so the reconcile operation can run
        // instantly
        let new_addrs = client
            .resolve_addrs(context::current(), unresolved_addrs)
            .await
            .map_err(|e| ReconcileError2::RpcError(e.to_string()))?
            .map_err(ReconcileError2::AddressResolve)?;

        tracing::debug!(
            "resolved new addrs: {}",
            new_addrs
                .iter()
                .map(|(id, addr)| format!("{}: {}", id, addr))
                .collect::<Vec<_>>()
                .join(", ")
        );

        // Extend the cache with the new addresses
        let mut lock = state.resolved_addrs.write().await;
        lock.extend(new_addrs);
        if let Err(e) = state.db.set_resolved_addrs(Some(&lock)) {
            error!("failed to save resolved addrs to db: {e}");
        }

        Ok(ReconcileStatus::default())
    }
}

// TODO: large file download behavior (ledgers):
// same as above, except maybe chunk the downloads or

// TODO: support ledger.aleo.network snapshots:
// https://ledger.aleo.network/mainnet/snapshot/latest.txt
// https://ledger.aleo.network/testnet/snapshot/latest.txt
// https://ledger.aleo.network/canarynet/snapshot/latest.txt

impl EnvState {
    pub fn changed(&self, env_info: &EnvInfo) -> bool {
        env_info.storage.version != self.storage_version
            || env_info.storage.id != self.storage_id
            || env_info.network != self.network_id
    }
}

impl DataFormat for EnvState {
    type Header = (u8, DataHeaderOf<NetworkId>);

    const LATEST_HEADER: Self::Header = (1u8, NetworkId::LATEST_HEADER);

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        Ok(self.network_id.write_data(writer)?
            + self.storage_id.write_data(writer)?
            + self.storage_version.write_data(writer)?)
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if header.0 != Self::LATEST_HEADER.0 {
            return Err(snops_common::format::DataReadError::unsupported(
                "EnvIdentifier",
                Self::LATEST_HEADER.0,
                header.0,
            ));
        }

        Ok(Self {
            network_id: NetworkId::read_data(reader, &header.1)?,
            storage_id: StorageId::read_data(reader, &())?,
            storage_version: u16::read_data(reader, &())?,
        })
    }
}
