use std::{
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use dashmap::DashMap;
use indexmap::IndexMap;
use reqwest::Url;
use snops_common::{
    api::EnvInfo,
    rpc::{agent::node::NodeServiceClient, control::ControlServiceClient, error::ReconcileError2},
    state::{AgentId, AgentPeer, AgentState, EnvId, TransferId, TransferStatus},
    util::OpaqueDebug,
};
use tarpc::context;
use tokio::sync::{mpsc::Sender, oneshot, RwLock};
use tracing::error;

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
    pub queue_reconcile_tx: Sender<Instant>,
    pub env_info: RwLock<Option<(EnvId, Arc<EnvInfo>)>>,
    // Map of agent IDs to their resolved addresses.
    pub resolved_addrs: RwLock<IndexMap<AgentId, IpAddr>>,
    pub metrics: RwLock<Metrics>,

    pub transfer_tx: TransferTx,
    pub transfers: Arc<DashMap<TransferId, TransferStatus>>,

    pub node_client: RwLock<Option<NodeServiceClient>>,
    pub log_level_handler: ReloadHandler,
    /// A oneshot sender to shutdown the agent.
    pub shutdown: RwLock<Option<oneshot::Sender<()>>>,
}

impl GlobalState {
    pub fn is_ws_online(&self) -> bool {
        self.client.try_read().is_ok_and(|c| c.is_some())
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

    pub async fn queue_reconcile(&self, duration: Duration) -> bool {
        self.queue_reconcile_tx
            .try_send(Instant::now() + duration)
            .is_ok()
    }

    pub async fn set_env_info(&self, info: Option<(EnvId, Arc<EnvInfo>)>) {
        if let Err(e) = self.db.set_env_info(info.clone()) {
            error!("failed to save env info to db: {e}");
        }
        *self.env_info.write().await = info;
    }

    pub async fn get_env_info(&self, env_id: EnvId) -> Result<Arc<EnvInfo>, ReconcileError2> {
        match self.env_info.read().await.as_ref() {
            Some((id, info)) if *id == env_id => return Ok(info.clone()),
            _ => {}
        }

        let client = self
            .client
            .read()
            .await
            .clone()
            .ok_or(ReconcileError2::Offline)?;

        let info = client
            .get_env_info(context::current(), env_id)
            .await
            .map_err(|e| ReconcileError2::RpcError(e.to_string()))?
            .ok_or(ReconcileError2::MissingEnv(env_id))?;

        let env_info = (env_id, Arc::new(info));
        if let Err(e) = self.db.set_env_info(Some(env_info.clone())) {
            error!("failed to save env info to db: {e}");
        }
        *self.env_info.write().await = Some(env_info.clone());

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

    pub async fn is_node_online(&self) -> bool {
        self.node_client.read().await.is_some()
    }

    pub async fn get_node_client(&self) -> Option<NodeServiceClient> {
        self.node_client.read().await.clone()
    }

    pub async fn update_agent_state(&self, state: AgentState, env_info: Option<(EnvId, EnvInfo)>) {
        self.set_env_info(env_info.map(|(id, e)| (id, Arc::new(e))))
            .await;
        if let Err(e) = self.db.set_agent_state(&state) {
            error!("failed to save agent state to db: {e}");
        }
        let state = Arc::new(state);
        *self.agent_state.write().await = state;

        // Queue a reconcile to apply the new state
        self.queue_reconcile(Duration::ZERO).await;
    }
}
