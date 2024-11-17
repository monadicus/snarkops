use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

use snops_common::{
    api::EnvInfo,
    binaries::{BinaryEntry, BinarySource},
    constant::SNARKOS_FILE,
    rpc::error::ReconcileError2,
    state::{
        AgentId, AgentPeer, AgentState, InternedId, NetworkId, NodeState, StorageId, TransferId,
    },
};
use tarpc::context;
use tracing::{error, trace, warn};

use super::{
    command::NodeCommand, default_binary, FileReconciler, Reconcile, ReconcileCondition,
    ReconcileStatus,
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
    /// Metadata about an active ledger transfer
    ledger_transfer: Option<TransferId>,
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

                // Resolve the node's binary
                // TODO: move into BinaryReconciler
                'binary: {
                    // Binary entry for the node
                    let default_binary = default_binary(&env_info);
                    let target_binary = env_info
                        .storage
                        .binaries
                        .get(&node.binary.unwrap_or_default())
                        .unwrap_or(&default_binary);

                    // Check if the binary has changed
                    let binary_has_changed = transfers
                        .binary_transfer
                        .as_ref()
                        .map(|(_, b)| b != target_binary)
                        .unwrap_or(true);
                    let binary_is_ok = transfers
                        .binary_ok_at
                        .map(|ok| ok.elapsed().as_secs() < 300) // check if the binary has been OK for 5 minutes
                        .unwrap_or(false);

                    // If the binary has not changed and has not expired, we can skip the binary
                    // reconciler
                    if !binary_has_changed && binary_is_ok {
                        break 'binary;
                    }

                    let src = match &target_binary.source {
                        BinarySource::Url(url) => url.clone(),
                        BinarySource::Path(path) => {
                            let url = format!("{}{}", &self.state.endpoint, path.display());
                            url.parse::<reqwest::Url>()
                                .map_err(|e| ReconcileError2::UrlParseError(url, e.to_string()))?
                        }
                    };
                    let dst = self.state.cli.path.join(SNARKOS_FILE);

                    let is_api_offline = self.state.client.read().await.is_none();

                    let binary_res = FileReconciler::new(Arc::clone(&self.state), src, dst)
                        .with_offline(target_binary.is_api_file() && is_api_offline)
                        .with_binary(target_binary)
                        .with_tx_id(transfers.binary_transfer.as_ref().map(|(tx, _)| *tx))
                        .reconcile()
                        .await?;

                    // transfer is pending or a failure occurred
                    if binary_res.is_requeue() {
                        return Ok(binary_res.emptied().add_scope("binary_reconcile/requeue"));
                    }

                    match binary_res.inner {
                        // If the binary is OK, update the context
                        Some(true) => {
                            transfers.binary_ok_at = Some(Instant::now());
                        }
                        // If the binary is not OK, we will wait for the endpoint to come back
                        // online...
                        Some(false) => {
                            trace!(
                                "binary is not OK, waiting for the endpoint to come back online..."
                            );
                            return Ok(ReconcileStatus::empty()
                                .add_condition(ReconcileCondition::PendingConnection)
                                .add_scope("binary_reconcile/offline")
                                .requeue_after(Duration::from_secs(5)));
                        }
                        None => unreachable!("file reconciler returns a result when not requeued"),
                    }
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

                // TODO: download binaries
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
struct BinaryReconciler {
    state: Arc<GlobalState>,
    info: EnvInfo,
}

// TODO: binary reconcile behavior:
// 1. check if the file exists.
// 2. if not, start downloading the file
// 3. if the file is already downloading, requeue if not done
// 4. when the transfer is done, check the sha256 hash and size

// TODO: large file download behavior (ledgers):
// same as above, except maybe chunk the downloads or

// TODO: support ledger.aleo.network snapshots:
// https://ledger.aleo.network/mainnet/snapshot/latest.txt
// https://ledger.aleo.network/testnet/snapshot/latest.txt
// https://ledger.aleo.network/canarynet/snapshot/latest.txt
