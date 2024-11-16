use std::{collections::HashSet, ops::Deref, process::Stdio, sync::Arc};

use snops_common::{
    constant::{
        LEDGER_BASE_DIR, LEDGER_PERSIST_DIR, SNARKOS_FILE, SNARKOS_GENESIS_FILE, SNARKOS_LOG_FILE,
    },
    rpc::error::ReconcileError2,
    state::{AgentId, AgentPeer, AgentState, EnvId, KeyState, NodeState},
};
use tarpc::context;
use tokio::process::Command;
use tracing::{error, warn};

use super::{Reconcile, ReconcileStatus};
use crate::state::GlobalState;

struct AgentStateReconciler {
    agent_state: AgentState,
    state: Arc<GlobalState>,
}

impl Reconcile<(), ReconcileError2> for AgentStateReconciler {
    async fn reconcile(&self) -> Result<ReconcileStatus<()>, ReconcileError2> {
        match &self.agent_state {
            AgentState::Inventory => {
                // TODO: cleanup child process
                // TODO: cleanup other things
                return Ok(ReconcileStatus::empty());
            }
            AgentState::Node(env_id, node) => {
                // node is offline, no need to reconcile
                if !node.online {
                    return Ok(ReconcileStatus::empty());
                }

                let command_res = NodeCommandReconciler {
                    env_id: *env_id,
                    node: Arc::new(*node.clone()),
                    state: Arc::clone(&self.state),
                }
                .reconcile()
                .await?;

                if command_res.is_requeue() {
                    return Ok(command_res.emptied());
                }

                let Some(_command) = command_res.take() else {
                    return Ok(ReconcileStatus::default());
                };

                // TODO: spawn the command, manage its state
            }
        }

        Ok(ReconcileStatus::empty())
    }
}

struct NodeCommandReconciler {
    node: Arc<NodeState>,
    state: Arc<GlobalState>,
    env_id: EnvId,
}

impl Reconcile<Command, ReconcileError2> for NodeCommandReconciler {
    async fn reconcile(&self) -> Result<ReconcileStatus<Command>, ReconcileError2> {
        let NodeCommandReconciler {
            node,
            state,
            env_id,
        } = self;
        let info = state.get_env_info(*env_id).await?;

        // Resolve the addresses of the peers and validators
        let res = AddressResolveReconciler {
            node: Arc::clone(node),
            state: Arc::clone(state),
        }
        .reconcile()
        .await?;

        if res.is_requeue() {
            return Ok(res.emptied());
        }

        let mut command = Command::new(state.cli.path.join(SNARKOS_FILE));

        // set stdio
        if state.cli.quiet {
            command.stdout(Stdio::null());
        } else {
            command.stdout(std::io::stdout());
        }
        command.stderr(std::io::stderr());

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

        // add loki URL if one is set
        if let Some(loki) = state.loki.lock().unwrap().deref() {
            command
                .env(
                    "SNOPS_LOKI_LABELS",
                    format!("env_id={},node_key={}", env_id, node.node_key),
                )
                .arg("--loki")
                .arg(loki.as_str());
        }

        // setup the run command
        command
            .stderr(std::io::stderr())
            .envs(&node.env)
            .env("NETWORK", info.network.to_string())
            .env("HOME", &ledger_path)
            .arg("--log")
            .arg(state.cli.path.join(SNARKOS_LOG_FILE))
            .arg("run")
            .arg("--agent-rpc-port")
            .arg(state.agent_rpc_port.to_string())
            .arg("--type")
            .arg(node.node_key.ty.to_string())
            .arg("--ledger")
            .arg(ledger_path);

        if !info.storage.native_genesis {
            command
                .arg("--genesis")
                .arg(storage_path.join(SNARKOS_GENESIS_FILE));
        }

        // storage configuration
        command
            // port configuration
            .arg("--bind")
            .arg(state.cli.bind_addr.to_string())
            .arg("--bft")
            .arg(state.cli.ports.bft.to_string())
            .arg("--rest")
            .arg(state.cli.ports.rest.to_string())
            .arg("--metrics")
            .arg(state.cli.ports.metrics.to_string())
            .arg("--node")
            .arg(state.cli.ports.node.to_string());

        match &node.private_key {
            KeyState::None => {}
            KeyState::Local => {
                command.arg("--private-key-file").arg(
                    state
                        .cli
                        .private_key_file
                        .as_ref()
                        .ok_or(ReconcileError2::MissingLocalPrivateKey)?,
                );
            }
            KeyState::Literal(pk) => {
                command.arg("--private-key").arg(pk);
            }
        }

        // conditionally add retention policy
        if let Some(policy) = &info.storage.retention_policy {
            command.arg("--retention-policy").arg(policy.to_string());
        }

        if !node.peers.is_empty() {
            command
                .arg("--peers")
                .arg(state.agentpeers_to_cli(&node.peers).await.join(","));
        }

        if !node.validators.is_empty() {
            command
                .arg("--validators")
                .arg(state.agentpeers_to_cli(&node.validators).await.join(","));
        }

        Ok(ReconcileStatus::new(Some(command)))
    }
}

struct AddressResolveReconciler {
    state: Arc<GlobalState>,
    node: Arc<NodeState>,
}

impl Reconcile<(), ReconcileError2> for AddressResolveReconciler {
    async fn reconcile(&self) -> Result<ReconcileStatus<()>, ReconcileError2> {
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
