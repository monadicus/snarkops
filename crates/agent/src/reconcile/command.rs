use std::{net::IpAddr, ops::Deref, path::PathBuf, process::Stdio, sync::Arc};

use indexmap::IndexMap;
use snops_checkpoint::RetentionPolicy;
use snops_common::{
    api::AgentEnvInfo,
    constant::{
        LEDGER_BASE_DIR, LEDGER_PERSIST_DIR, NODE_DATA_DIR, SNARKOS_FILE, SNARKOS_GENESIS_FILE,
        SNARKOS_LOG_FILE,
    },
    rpc::error::ReconcileError,
    state::{EnvId, KeyState, NetworkId, NodeKey, NodeState, PortConfig},
};
use tokio::process::Command;
use url::Url;

use crate::state::GlobalState;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NodeCommand {
    /// Path to the snarkos binary
    pub command_path: PathBuf,
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
    pub async fn new(
        state: Arc<GlobalState>,
        node: Arc<NodeState>,
        env_id: EnvId,
        env_info: Arc<AgentEnvInfo>,
    ) -> Result<Self, ReconcileError> {
        let storage_path = state
            .cli
            .storage_path(env_info.network, env_info.storage.id);

        let ledger_path = if env_info.storage.persist {
            storage_path.join(LEDGER_PERSIST_DIR)
        } else {
            let mut dir = state.cli.path.join(NODE_DATA_DIR);
            dir.push(LEDGER_BASE_DIR);
            dir
        };

        Ok(NodeCommand {
            command_path: state.cli.path.join(SNARKOS_FILE),
            quiet: state.cli.quiet,
            env_id,
            node_key: node.node_key.clone(),
            loki: state.loki.lock().ok().and_then(|l| l.deref().clone()),
            ledger_path,
            log_path: state.cli.path.join(SNARKOS_LOG_FILE),
            genesis_path: (!env_info.storage.native_genesis)
                .then(|| storage_path.join(SNARKOS_GENESIS_FILE)),
            network: env_info.network,
            env: node.env.clone(),
            agent_rpc_port: state.agent_rpc_port,
            bind_addr: state.cli.bind_addr,
            ports: state.cli.ports,
            private_key: if let KeyState::Literal(pk) = &node.private_key {
                Some(pk.clone())
            } else {
                None
            },
            // Ensure the private key file can be resolved.
            // This is only reachable when an agent is referred to by its
            // id in an environment spec.
            private_key_file: if let KeyState::Local = &node.private_key {
                Some(
                    state
                        .cli
                        .private_key_file
                        .clone()
                        .ok_or(ReconcileError::MissingLocalPrivateKey)?,
                )
            } else {
                None
            },
            peers: state.agentpeers_to_cli(&node.peers).await,
            validators: state.agentpeers_to_cli(&node.validators).await,
            retention_policy: env_info.storage.retention_policy.clone(),
        })
    }

    pub fn build(&self) -> Command {
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
