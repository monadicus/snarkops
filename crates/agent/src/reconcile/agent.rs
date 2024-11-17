use std::{
    collections::HashSet,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::stream::AbortHandle;
use snops_common::{
    api::EnvInfo,
    binaries::{BinaryEntry, BinarySource},
    constant::{SNARKOS_FILE, VERSION_FILE},
    rpc::error::ReconcileError2,
    state::{
        AgentId, AgentPeer, AgentState, InternedId, NetworkId, NodeState, StorageId, TransferId,
    },
};
use tarpc::context;
use tokio::sync::{Mutex, Semaphore};
use tracing::{error, trace, warn};

use super::{
    command::NodeCommand, default_binary, get_version_from_path, DirectoryReconciler,
    FileReconciler, Reconcile, ReconcileCondition, ReconcileStatus,
};
use crate::state::GlobalState;

/// Attempt to reconcile the agent's current state.
/// This will download files and start/stop the node
pub struct AgentStateReconciler {
    pub agent_state: Arc<AgentState>,
    pub state: Arc<GlobalState>,
    pub context: AgentStateReconcilerContext,
}

type LedgerModifyResult = Result<bool, ReconcileError2>;

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
    /// Metadata about an active ledger transfer
    ledger_transfer: Option<TransferId>,
    /// A handle containing the task that modifies the ledger.
    /// The mutex is held until the task is complete, and the bool is set to
    /// true when the task is successful.
    ledger_modify_handle: Option<(AbortHandle, Arc<Mutex<Option<LedgerModifyResult>>>)>,
    /// A handle containing the task that unzips the ledger.
    /// The mutex is held until the task is complete, and the bool is set to
    /// true when the task is successful.
    ledger_unpack_handle: Option<(AbortHandle, Arc<Mutex<Option<LedgerModifyResult>>>)>,
    /// Time the ledger was marked as OK
    ledger_ok_at: Option<Instant>,
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
    /// All parameters needed to build the command to start the node
    command: Option<NodeCommand>,
    /// Information about active transfers
    transfers: Option<TransfersContext>,
    // TODO: allow transfers to be interrupted. potentially allow them to be resumed by using the
    // file range feature.
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

                // TODO: check if addrs have changed, and update shutdown_pending

                // node is offline, no need to reconcile
                if !node.online {
                    // TODO: tear down the node if it is running
                    return Ok(ReconcileStatus::default().add_scope("agent_state/node/offline"));
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
                StorageVersionReconciler(&storage_path, env_info.storage.version)
                    .reconcile()
                    .await?;

                // Create the storage path if it does not exist
                DirectoryReconciler(&storage_path).reconcile().await?;

                // Resolve the node's binary
                let binary_res = BinaryReconciler {
                    state: Arc::clone(&self.state),
                    env_info: Arc::clone(&env_info),
                    node_binary: node.binary,
                    binary_transfer: &mut transfers.binary_transfer,
                    binary_ok_at: &mut transfers.binary_ok_at,
                }
                .reconcile()
                .await?;

                if binary_res.is_requeue() {
                    return Ok(binary_res.add_scope("binary_reconcile/requeue"));
                }

                // Resolve the addresses of the peers and validators
                // TODO: Set an expiry for resolved addresses
                let addr_res = AddressResolveReconciler {
                    node: Arc::clone(&node_arc),
                    state: Arc::clone(&self.state),
                }
                .reconcile()
                .await?;

                if addr_res.is_requeue() {
                    return Ok(addr_res.add_scope("address_resolve/requeue"));
                }

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

                if self.context.command.as_ref() != Some(&command) {
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

/// Download a specific binary file needed to run the node
struct BinaryReconciler<'a> {
    state: Arc<GlobalState>,
    env_info: Arc<EnvInfo>,
    node_binary: Option<InternedId>,
    /// Metadata about an active binary transfer
    binary_transfer: &'a mut Option<(TransferId, BinaryEntry)>,
    /// Time the binary was marked as OK
    binary_ok_at: &'a mut Option<Instant>,
}

impl<'a> Reconcile<(), ReconcileError2> for BinaryReconciler<'a> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError2> {
        let BinaryReconciler {
            state,
            env_info,
            node_binary,
            binary_transfer,
            binary_ok_at,
        } = self;

        // Binary entry for the node
        let default_binary = default_binary(env_info);
        let target_binary = env_info
            .storage
            .binaries
            .get(&node_binary.unwrap_or_default())
            .unwrap_or(&default_binary);

        // Check if the binary has changed
        let binary_has_changed = binary_transfer
            .as_ref()
            .map(|(_, b)| b != target_binary)
            .unwrap_or(true);
        let binary_is_ok = binary_ok_at
            .map(|ok| ok.elapsed().as_secs() < 300) // check if the binary has been OK for 5 minutes
            .unwrap_or(false);

        // If the binary has not changed and has not expired, we can skip the binary
        // reconciler
        if !binary_has_changed && binary_is_ok {
            return Ok(ReconcileStatus::default());
        }

        let src = match &target_binary.source {
            BinarySource::Url(url) => url.clone(),
            BinarySource::Path(path) => {
                let url = format!("{}{}", &state.endpoint, path.display());
                url.parse::<reqwest::Url>()
                    .map_err(|e| ReconcileError2::UrlParseError(url, e.to_string()))?
            }
        };
        let dst = state.cli.path.join(SNARKOS_FILE);

        let is_api_offline = state.client.read().await.is_none();

        let file_res = FileReconciler::new(Arc::clone(state), src, dst)
            .with_offline(target_binary.is_api_file() && is_api_offline)
            .with_binary(target_binary)
            .with_tx_id(binary_transfer.as_ref().map(|(tx, _)| *tx))
            .reconcile()
            .await?;

        // transfer is pending or a failure occurred
        if file_res.is_requeue() {
            return Ok(file_res.emptied().add_scope("file_reconcile/requeue"));
        }

        match file_res.inner {
            // If the binary is OK, update the context
            Some(true) => {
                **binary_ok_at = Some(Instant::now());
                Ok(ReconcileStatus::default())
            }
            // If the binary is not OK, we will wait for the endpoint to come back
            // online...
            Some(false) => {
                trace!("binary is not OK, waiting for the endpoint to come back online...");
                Ok(ReconcileStatus::empty()
                    .add_condition(ReconcileCondition::PendingConnection)
                    .add_scope("agent_state/binary/offline")
                    .requeue_after(Duration::from_secs(5)))
            }
            None => unreachable!("file reconciler returns a result when not requeued"),
        }
    }
}

struct StorageVersionReconciler<'a>(&'a Path, u16);

impl<'a> Reconcile<(), ReconcileError2> for StorageVersionReconciler<'a> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError2> {
        let StorageVersionReconciler(path, version) = self;

        let version_file = path.join(VERSION_FILE);

        let version_file_data = if !version_file.exists() {
            None
        } else {
            tokio::fs::read_to_string(&version_file)
                .await
                .map_err(|e| ReconcileError2::FileReadError(version_file.clone(), e.to_string()))?
                .parse()
                .ok()
        };

        // wipe old storage when the version changes
        Ok(if version_file_data != Some(*version) && path.exists() {
            let _ = tokio::fs::remove_dir_all(&path).await;
            ReconcileStatus::default()
        } else {
            // return an empty status if the version is the same
            ReconcileStatus::empty()
        })
    }
}

// TODO: large file download behavior (ledgers):
// same as above, except maybe chunk the downloads or

// TODO: support ledger.aleo.network snapshots:
// https://ledger.aleo.network/mainnet/snapshot/latest.txt
// https://ledger.aleo.network/testnet/snapshot/latest.txt
// https://ledger.aleo.network/canarynet/snapshot/latest.txt

// TODO: some kind of reconciler iterator that attempts to reconcile a chain
// until hitting a requeue
