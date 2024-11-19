use std::{collections::HashSet, sync::Arc, time::Instant};

use futures::stream::AbortHandle;
use snops_common::{
    api::EnvInfo,
    binaries::BinaryEntry,
    rpc::error::ReconcileError2,
    state::{
        AgentId, AgentPeer, AgentState, HeightRequest, NetworkId, NodeState, StorageId, TransferId,
    },
};
use tarpc::context;
use tokio::sync::Mutex;
use tracing::{error, warn};

use super::{
    command::NodeCommand,
    process::ProcessContext,
    storage::{BinaryReconciler, GenesisReconciler, LedgerModifyResult, StorageVersionReconciler},
    DirectoryReconciler, Reconcile, ReconcileStatus,
};
use crate::state::GlobalState;

/// Attempt to reconcile the agent's current state.
/// This will download files and start/stop the node
pub struct AgentStateReconciler {
    pub agent_state: Arc<AgentState>,
    pub state: Arc<GlobalState>,
    pub context: AgentStateReconcilerContext,
}

#[derive(Default)]
struct TransfersContext {
    // TODO: persist network_id, storage_id, and storage_version
    network_id: NetworkId,
    storage_id: StorageId,
    storage_version: u16,

    /// Metadata about an active binary transfer
    binary_transfer: Option<(TransferId, BinaryEntry)>,
    /// Time the binary was marked as OK
    binary_ok_at: Option<Instant>,

    /// Metadata about an active genesis block transfer
    genesis_transfer: Option<TransferId>,
    /// Time the genesis block was marked as OK
    genesis_ok_at: Option<Instant>,

    /// The last ledger height that was successfully configured
    ledger_last_height: Option<HeightRequest>,
    /// The height that is currently being configured
    ledger_pending_height: Option<HeightRequest>,

    /// Metadata about an active ledger tar file transfer
    ledger_transfer: Option<TransferId>,
    /// Time the ledger tar file was marked as OK
    ledger_ok_at: Option<Instant>,
    /// A handle containing the task that modifies the ledger.
    /// The mutex is held until the task is complete, and the bool is set to
    /// true when the task is successful.
    ledger_modify_handle: Option<(AbortHandle, Arc<Mutex<Option<LedgerModifyResult>>>)>,
    /// A handle containing the task that unzips the ledger.
    /// The mutex is held until the task is complete, and the bool is set to
    /// true when the task is successful.
    ledger_unpack_handle: Option<(AbortHandle, Arc<Mutex<Option<LedgerModifyResult>>>)>,
}

impl TransfersContext {
    pub fn changed(&self, env_info: &EnvInfo) -> bool {
        env_info.storage.version != self.storage_version
            || env_info.storage.id != self.storage_id
            || env_info.network != self.network_id
    }
}

#[derive(Default)]
pub struct AgentStateReconcilerContext {
    // TODO: allow transfers to be interrupted. potentially allow them to be resumed by using the
    // file range feature.
    /// Information about active transfers
    transfers: Option<TransfersContext>,
    /// Information about the node process
    process: Option<ProcessContext>,
}

/// Run a reconciler and return early if a requeue is needed. A condition is
/// added to the scope when a requeue is needed to provide more context when
/// monitoring the agent.
macro_rules! reconcile {
    ($id:ident, $e:expr) => {
        let res = $e.reconcile().await?;
        if res.is_requeue() {
            return Ok(res.add_scope(concat!(stringify!($id), "/requeue")));
        }
    };
}

impl Reconcile<(), ReconcileError2> for AgentStateReconciler {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError2> {
        match self.agent_state.as_ref() {
            AgentState::Inventory => {
                // TODO: cleanup child process
                // TODO: cleanup other things

                return Ok(ReconcileStatus::default().add_scope("agent_state/inventory"));
            }
            AgentState::Node(env_id, node) => {
                let env_info = self.state.get_env_info(*env_id).await?;

                // Check if the storage version, storage id, or network id has changed
                let storage_has_changed = self
                    .context
                    .transfers
                    .as_ref()
                    .map(|t| t.changed(&env_info))
                    .unwrap_or(true);

                // If the node should be torn down, or the storage has changed, we need to
                // gracefully shut down the node.
                let shutdown_pending = !node.online || storage_has_changed;

                if let (true, Some(process)) = (shutdown_pending, self.context.process.as_ref()) {
                    // TODO: reconcile process destruction
                }

                // TODO: check if addrs have changed, and update shutdown_pending

                // node is offline, no need to reconcile
                if !node.online {
                    // TODO: tear down the node if it is running
                    return Ok(ReconcileStatus::default().add_scope("agent_state/offline"));
                }

                let node_arc = Arc::new(*node.clone());

                if storage_has_changed {
                    // TODO: abort any ongoing transfers, then requeue
                }

                // initialize the transfers context with the current status
                if self.context.transfers.is_none() {
                    // TODO: write this to the db
                    self.context.transfers = Some(TransfersContext {
                        network_id: env_info.network,
                        storage_id: env_info.storage.id,
                        storage_version: env_info.storage.version,
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

                // Resolve the addresses of the peers and validators
                // TODO: Set an expiry for resolved addresses
                reconcile!(
                    address_resolve,
                    AddressResolveReconciler {
                        node: Arc::clone(&node_arc),
                        state: Arc::clone(&self.state),
                    }
                );

                // TODO: restart the node if the binaries changed. this means storing the hashes
                // of the downloaded files

                // TODO: requeue if the binaries are not ready

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
                let _cmd = command.build();
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
