use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
};

use reqwest::Url;
use snops_common::{
    api::StorageInfo,
    rpc::control::ControlServiceClient,
    state::{AgentId, AgentPeer, AgentState, EnvId},
};
use tokio::{
    process::Child,
    sync::{Mutex as AsyncMutex, RwLock},
    task::AbortHandle,
};

use crate::{api, cli::Cli, metrics::Metrics};

pub type AppState = Arc<GlobalState>;

/// Global state for this agent runner.
pub struct GlobalState {
    pub client: ControlServiceClient,

    pub external_addr: Option<IpAddr>,
    pub internal_addrs: Vec<IpAddr>,
    pub cli: Cli,
    pub endpoint: String,
    pub jwt: Mutex<Option<String>>,
    pub loki: Mutex<Option<Url>>,
    pub agent_state: RwLock<AgentState>,
    pub env_to_storage: RwLock<HashMap<EnvId, StorageInfo>>,
    pub reconcilation_handle: AsyncMutex<Option<AbortHandle>>,
    pub child: RwLock<Option<Child>>, /* TODO: this may need to be handled by an owning thread,
                                       * not sure yet */
    // Map of agent IDs to their resolved addresses.
    pub resolved_addrs: RwLock<HashMap<AgentId, IpAddr>>,
    pub metrics: RwLock<Metrics>,
}

impl GlobalState {
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

    pub async fn get_env_info(&self, env_id: EnvId) -> anyhow::Result<StorageInfo> {
        if let Some(info) = self.env_to_storage.read().await.get(&env_id).cloned() {
            return Ok(info);
        }

        // if an else was used here, the lock would be held for the entire function so
        // we return early to prevent a deadlock

        let info = api::get_storage_info(format!(
            "http://{}/api/v1/env/{env_id}/storage",
            &self.endpoint
        ))
        .await?;

        self.env_to_storage
            .write()
            .await
            .insert(env_id, info.clone());

        Ok(info)
    }
}
