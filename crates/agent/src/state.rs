use std::{
    collections::HashSet,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use dashmap::DashMap;
use indexmap::IndexMap;
use reqwest::Url;
use snops_common::{
    api::AgentEnvInfo,
    rpc::{agent::node::NodeServiceClient, control::ControlServiceClient, error::ReconcileError},
    state::{
        snarkos_status::SnarkOSStatus, AgentId, AgentPeer, AgentState, EnvId, ReconcileOptions,
        TransferId, TransferStatus,
    },
    util::OpaqueDebug,
};
use tarpc::context;
use tokio::sync::{mpsc::Sender, oneshot, RwLock};
use tracing::{error, info};

use crate::{cli::Cli, db::Database, log::ReloadHandler, metrics::Metrics, transfers::TransferTx};

pub const NODE_GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

pub type AppState = Arc<GlobalState>;
pub type ClientLock = Arc<RwLock<Option<ControlServiceClient>>>;

/// Global state for this agent runner.
pub struct GlobalState {
    pub client: ClientLock,
    pub db: OpaqueDebug<Database>,
    pub _started: Instant,

    pub external_addr: Option<IpAddr>,
    pub internal_addrs: Vec<IpAddr>,
    pub agent_rpc_port: u16,
    pub cli: Cli,
    pub endpoint: String,
    pub loki: Mutex<Option<Url>>,
    /// Desired state the agent should be in. After each reconciliation, the
    /// agent will attempt to transition to this state.
    pub agent_state: RwLock<Arc<AgentState>>,
    /// A sender for emitting the next time to reconcile the agent.
    /// Helpful for scheduling the next reconciliation.
    pub queue_reconcile_tx: Sender<(Instant, ReconcileOptions)>,
    pub env_info: RwLock<Option<(EnvId, Arc<AgentEnvInfo>)>>,
    // Map of agent IDs to their resolved addresses.
    pub resolved_addrs: RwLock<IndexMap<AgentId, IpAddr>>,
    pub metrics: RwLock<Metrics>,

    pub transfer_tx: TransferTx,
    pub transfers: Arc<DashMap<TransferId, TransferStatus>>,

    pub node_client: RwLock<Option<NodeServiceClient>>,
    pub last_node_status: RwLock<Option<(Instant, SnarkOSStatus)>>,
    pub log_level_handler: ReloadHandler,
    /// A oneshot sender to shutdown the agent.
    pub shutdown: RwLock<Option<oneshot::Sender<()>>>,
}

impl GlobalState {
    pub fn is_ws_online(&self) -> bool {
        self.client.try_read().is_ok_and(|c| c.is_some())
    }

    pub async fn get_ws_client(&self) -> Option<ControlServiceClient> {
        self.client.read().await.clone()
    }

    pub async fn get_agent_state(&self) -> Arc<AgentState> {
        self.agent_state.read().await.clone()
    }

    // Resolve the addresses of the given agents.
    // Locks resolve_addrs
    pub async fn agentpeers_to_cli(&self, peers: &[AgentPeer]) -> Vec<String> {
        let resolved_addrs = self.resolved_addrs.read().await;
        peers
            .iter()
            .filter_map(|p| match p {
                AgentPeer::Internal(id, port) => resolved_addrs
                    .get(id)
                    .copied()
                    .map(|addr| std::net::SocketAddr::new(addr, *port).to_string()),
                AgentPeer::External(addr) => Some(addr.to_string()),
            })
            .collect::<Vec<_>>()
    }

    pub async fn queue_reconcile(&self, duration: Duration, opts: ReconcileOptions) -> bool {
        self.queue_reconcile_tx
            .try_send((Instant::now() + duration, opts))
            .is_ok()
    }

    pub async fn set_env_info(&self, info: Option<(EnvId, Arc<AgentEnvInfo>)>) {
        if let Err(e) = self.db.set_env_info(info.clone()) {
            error!("failed to save env info to db: {e}");
        }
        *self.env_info.write().await = info;
    }

    /// Fetch the environment info for the given env_id, caching the result.
    pub async fn get_env_info(&self, env_id: EnvId) -> Result<Arc<AgentEnvInfo>, ReconcileError> {
        match self.env_info.read().await.as_ref() {
            Some((id, info)) if *id == env_id => return Ok(info.clone()),
            _ => {}
        }

        let client = self
            .client
            .read()
            .await
            .clone()
            .ok_or(ReconcileError::Offline)?;

        let info = client
            .get_env_info(context::current(), env_id)
            .await
            .map_err(|e| ReconcileError::RpcError(e.to_string()))?
            .ok_or(ReconcileError::MissingEnv(env_id))?;

        let env_info = (env_id, Arc::new(info));
        if let Err(e) = self.db.set_env_info(Some(env_info.clone())) {
            error!("failed to save env info to db: {e}");
        }
        *self.env_info.write().await = Some(env_info.clone());

        // clear the resolved addrs cache when the env info changes
        self.resolved_addrs.write().await.clear();
        if let Err(e) = self.db.set_resolved_addrs(None) {
            error!("failed to save resolved addrs to db: {e}");
        }

        Ok(env_info.1)
    }

    pub fn transfer_tx(&self) -> TransferTx {
        self.transfer_tx.clone()
    }

    pub async fn shutdown(&self) {
        if let Some(tx) = self.shutdown.write().await.take() {
            let _ = tx.send(());
        }
    }

    pub fn is_node_online(&self) -> bool {
        self.node_client.try_read().is_ok_and(|c| c.is_some())
    }

    pub async fn get_node_client(&self) -> Option<NodeServiceClient> {
        self.node_client.read().await.clone()
    }

    pub async fn update_agent_state(&self, state: AgentState, opts: ReconcileOptions) {
        if let Err(e) = self.db.set_agent_state(&state) {
            error!("failed to save agent state to db: {e}");
        }
        let state = Arc::new(state);
        *self.agent_state.write().await = state;

        // Queue a reconcile to apply the new state
        self.queue_reconcile(Duration::ZERO, opts).await;
    }

    pub async fn re_fetch_peer_addrs(&self) {
        let agent_state = self.get_agent_state().await;
        let AgentState::Node(_, node) = agent_state.as_ref() else {
            return;
        };

        let Some(client) = self.get_ws_client().await else {
            return;
        };

        let peer_ids = node
            .peers
            .iter()
            .chain(node.validators.iter())
            .filter_map(|p| {
                if let snops_common::state::AgentPeer::Internal(id, _) = p {
                    Some(*id)
                } else {
                    None
                }
            })
            // Ensure we only have unique agent ids (can use itertools down the line)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        if peer_ids.is_empty() {
            return;
        }

        let new_addrs = match client.resolve_addrs(context::current(), peer_ids).await {
            Ok(Ok(new_addrs)) => new_addrs,
            Ok(Err(e)) => {
                error!("Control plane failed to resolve addresses: {e}");
                return;
            }
            Err(e) => {
                error!("RPC failed to resolve addresses: {e}");
                return;
            }
        };

        // Extend the cache with the updated addrs
        let mut lock = self.resolved_addrs.write().await;
        let has_new_addr = new_addrs
            .iter()
            .any(|(id, addr)| lock.get(id) != Some(addr));

        if !has_new_addr {
            return;
        }

        info!("Resolved updated addrs from handshake");

        lock.extend(new_addrs);
        if let Err(e) = self.db.set_resolved_addrs(Some(&lock)) {
            error!("failed to save resolved addrs to db: {e}");
        }
    }

    pub async fn set_node_status(&self, status: Option<SnarkOSStatus>) {
        *self.last_node_status.write().await = status.map(|s| (Instant::now(), s));
    }

    pub async fn get_node_status(&self) -> Option<SnarkOSStatus> {
        self.last_node_status.read().await.clone().map(|(_, s)| s)
    }
}
