use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use snops_common::{
    api::StorageInfo,
    rpc::control::ControlServiceClient,
    state::{AgentId, AgentPeer, AgentState, EnvId},
};
use tokio::{
    process::Child,
    select,
    sync::{Mutex as AsyncMutex, RwLock},
    task::AbortHandle,
};
use tracing::info;

use crate::{api, cli::Cli, metrics::Metrics};

pub const NODE_GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

pub type AppState = Arc<GlobalState>;

/// Global state for this agent runner.
pub struct GlobalState {
    pub client: ControlServiceClient,

    pub external_addr: Option<IpAddr>,
    pub internal_addrs: Vec<IpAddr>,
    pub cli: Cli,
    pub endpoint: String,
    pub jwt: Mutex<Option<String>>,
    pub agent_state: RwLock<AgentState>,
    pub env_info: RwLock<Option<(EnvId, StorageInfo)>>,
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
        match self.env_info.read().await.as_ref() {
            Some((id, info)) if *id == env_id => return Ok(info.clone()),
            _ => {}
        }

        let info = api::get_storage_info(format!("{}/api/v1/env/{env_id}/storage", &self.endpoint))
            .await?;

        *self.env_info.write().await = Some((env_id, info.clone()));

        Ok(info)
    }

    /// Attempt to gracefully shutdown the node if one is running.
    pub async fn node_graceful_shutdown(&self) {
        if let Some((mut child, id)) = self.child.write().await.take().and_then(|ch| {
            let id = ch.id()?;
            Some((ch, id))
        }) {
            use nix::{
                sys::signal::{self, Signal},
                unistd::Pid,
            };

            // send SIGINT to the child process
            signal::kill(Pid::from_raw(id as i32), Signal::SIGINT).unwrap();

            // wait for graceful shutdown or kill process after 10 seconds
            let timeout = tokio::time::sleep(NODE_GRACEFUL_SHUTDOWN_TIMEOUT);

            select! {
                _ = child.wait() => (),
                _ = timeout => {
                    info!("snarkos process did not gracefully shut down, killing...");
                    child.kill().await.unwrap();
                }
            }
        }
    }
}
