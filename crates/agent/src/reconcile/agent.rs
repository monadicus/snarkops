use std::{
    collections::HashSet, net::IpAddr, ops::Deref, path::PathBuf, process::Stdio, sync::Arc,
};

use indexmap::IndexMap;
use snops_checkpoint::RetentionPolicy;
use snops_common::{
    api::EnvInfo,
    constant::{
        LEDGER_BASE_DIR, LEDGER_PERSIST_DIR, SNARKOS_FILE, SNARKOS_GENESIS_FILE, SNARKOS_LOG_FILE,
    },
    rpc::error::ReconcileError2,
    state::{
        AgentId, AgentPeer, AgentState, EnvId, InternedId, KeyState, NetworkId, NodeKey, NodeState,
        PortConfig,
    },
};
use tarpc::context;
use tokio::process::Command;
use tracing::{error, warn};
use url::Url;

use super::{Reconcile, ReconcileStatus};
use crate::state::GlobalState;

/// Attempt to reconcile the agent's current state.
/// This will download files and start/stop the node
pub struct AgentStateReconciler {
    pub agent_state: Arc<AgentState>,
    pub state: Arc<GlobalState>,
    pub context: AgentStateReconcilerContext,
}

#[derive(Default)]
pub struct AgentStateReconcilerContext {
    /// All parameters needed to build the command to start the node
    command: Option<NodeCommand>,
    // TODO: store active transfers here for monitoring
    // TODO: update api::download_file to receive a transfer id
}

impl Reconcile<AgentStateReconcilerContext, ReconcileError2> for AgentStateReconciler {
    async fn reconcile(
        self,
    ) -> Result<ReconcileStatus<AgentStateReconcilerContext>, ReconcileError2> {
        match self.agent_state.as_ref() {
            AgentState::Inventory => {
                // TODO: cleanup child process
                // TODO: cleanup other things

                // return a default context because the node, in inventory, has no state
                return Ok(ReconcileStatus::default().add_scope("agent_state/inventory"));
            }
            AgentState::Node(env_id, node) => {
                // node is offline, no need to reconcile
                if !node.online {
                    // TODO: tear down the node if it is running
                    return Ok(
                        ReconcileStatus::with(self.context).add_scope("agent_state/node/offline")
                    );
                }

                // TODO: download binaries
                // TODO: restart the node if the binaries changed. this means storing the hashes
                // of the downloaded files

                // TODO: requeue if the binaries are not ready

                let command_res = NodeCommandReconciler {
                    env_id: *env_id,
                    node: Arc::new(*node.clone()),
                    state: Arc::clone(&self.state),
                }
                .reconcile()
                .await?;

                if command_res.is_requeue() {
                    return Ok(command_res.emptied().add_scope("agent_state/node/requeue"));
                }

                let Some(command) = command_res.take() else {
                    return Ok(ReconcileStatus::default().add_scope("agent_state/node/no_command"));
                };

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

/// Given a node state, construct the command needed to start the node
struct NodeCommandReconciler {
    node: Arc<NodeState>,
    state: Arc<GlobalState>,
    env_id: EnvId,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct NodeCommand {
    /// Path to the snarkos binary
    command_path: PathBuf,
    /// If true, do not print stdout
    quiet: bool,
    /// Environment ID (used in loki)
    env_id: EnvId,
    /// Node key (drives NETWORK env)
    network: NetworkId,
    /// Node key (derives node type and loki)
    node_key: NodeKey,
    /// URL for sending logs to loki
    loki: Option<Url>,
    /// Path to the ledger directory
    ledger_path: PathBuf,
    /// Path to place the log file
    log_path: PathBuf,
    /// Path to genesis block. When absent, use the network's genesis block.
    genesis_path: Option<PathBuf>,
    /// Env variables to pass to the node
    env: IndexMap<String, String>,
    /// Port to bind the agent's RPC server for node status
    agent_rpc_port: u16,
    /// Address to bind the node to
    bind_addr: IpAddr,
    /// Port configuration for the node
    ports: PortConfig,
    /// Private key to use for the node
    private_key: Option<String>,
    /// Path to a file containing the private key
    private_key_file: Option<PathBuf>,
    /// Retention policy for the node
    retention_policy: Option<RetentionPolicy>,
    /// Resolved peer addresses for the node
    peers: Vec<String>,
    /// Resolved validator addresses for the node
    validators: Vec<String>,
}

impl NodeCommand {
    fn build(&self) -> Command {
        let mut command = Command::new(&self.command_path);

        // set stdio
        if self.quiet {
            command.stdout(Stdio::null());
        } else {
            command.stdout(std::io::stdout());
        }
        command.stderr(std::io::stderr());

        // add loki URL if one is set
        if let Some(loki) = &self.loki {
            command
                .env(
                    "SNOPS_LOKI_LABELS",
                    format!("env_id={},node_key={}", self.env_id, self.node_key),
                )
                .arg("--loki")
                .arg(loki.as_str());
        }

        // setup the run command
        command
            .stderr(std::io::stderr())
            .envs(&self.env)
            .env("NETWORK", self.network.to_string())
            .env("HOME", &self.ledger_path)
            .arg("--log")
            .arg(&self.log_path)
            .arg("run")
            .arg("--agent-rpc-port")
            .arg(self.agent_rpc_port.to_string())
            .arg("--type")
            .arg(self.node_key.ty.to_string())
            .arg("--ledger")
            .arg(&self.ledger_path);

        if let Some(genesis) = &self.genesis_path {
            command.arg("--genesis").arg(genesis);
        }

        // storage configuration
        command
            // port configuration
            .arg("--bind")
            .arg(self.bind_addr.to_string())
            .arg("--bft")
            .arg(self.ports.bft.to_string())
            .arg("--rest")
            .arg(self.ports.rest.to_string())
            .arg("--metrics")
            .arg(self.ports.metrics.to_string())
            .arg("--node")
            .arg(self.ports.node.to_string());

        if let Some(pk) = &self.private_key {
            command.arg("--private-key").arg(pk);
        }

        if let Some(pk_file) = &self.private_key_file {
            command.arg("--private-key-file").arg(pk_file);
        }

        // conditionally add retention policy
        if let Some(policy) = &self.retention_policy {
            command.arg("--retention-policy").arg(policy.to_string());
        }

        if !self.peers.is_empty() {
            command.arg("--peers").arg(self.peers.join(","));
        }

        if !self.validators.is_empty() {
            command.arg("--validators").arg(self.validators.join(","));
        }

        command
    }
}

impl Reconcile<NodeCommand, ReconcileError2> for NodeCommandReconciler {
    async fn reconcile(self) -> Result<ReconcileStatus<NodeCommand>, ReconcileError2> {
        let NodeCommandReconciler {
            node,
            state,
            env_id,
        } = self;
        let info = state.get_env_info(env_id).await?;

        // Resolve the addresses of the peers and validators
        let res = AddressResolveReconciler {
            node: Arc::clone(&node),
            state: Arc::clone(&state),
        }
        .reconcile()
        .await?;

        if res.is_requeue() {
            return Ok(res
                .emptied()
                .add_scope("node_command/address_resolve/requeue"));
        }

        let storage_path = state
            .cli
            .path
            .join("storage")
            .join(info.network.to_string())
            .join(info.storage.id.to_string());

        let ledger_path = if info.storage.persist {
            storage_path.join(LEDGER_PERSIST_DIR)
        } else {
            state.cli.path.join(LEDGER_BASE_DIR)
        };

        let run = NodeCommand {
            command_path: state.cli.path.join(SNARKOS_FILE),
            quiet: state.cli.quiet,
            env_id,
            node_key: node.node_key.clone(),
            loki: state.loki.lock().ok().and_then(|l| l.deref().clone()),
            ledger_path,
            log_path: state.cli.path.join(SNARKOS_LOG_FILE),
            genesis_path: (!info.storage.native_genesis)
                .then(|| storage_path.join(SNARKOS_GENESIS_FILE)),
            network: info.network,
            env: node.env.clone(),
            agent_rpc_port: state.agent_rpc_port,
            bind_addr: state.cli.bind_addr,
            ports: state.cli.ports,
            private_key: if let KeyState::Literal(pk) = &node.private_key {
                Some(pk.clone())
            } else {
                None
            },
            private_key_file: if let KeyState::Local = &node.private_key {
                Some(
                    state
                        .cli
                        .private_key_file
                        .clone()
                        .ok_or(ReconcileError2::MissingLocalPrivateKey)?,
                )
            } else {
                None
            },
            peers: state.agentpeers_to_cli(&node.peers).await,
            validators: state.agentpeers_to_cli(&node.validators).await,
            retention_policy: info.storage.retention_policy.clone(),
        };

        Ok(ReconcileStatus::new(Some(run)))
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
    async fn reconcile(self) -> Result<ReconcileStatus<()>, ReconcileError2> {
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
    binary_id: Option<InternedId>,
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
