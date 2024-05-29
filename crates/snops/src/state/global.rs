use std::{collections::HashSet, net::SocketAddr, path::PathBuf, sync::Arc};

use dashmap::DashMap;
use prometheus_http_query::Client as PrometheusClient;
use snops_common::{
    constant::ENV_AGENT_KEY,
    state::{AgentId, AgentState, EnvId, NetworkId, StorageId},
};
use tokio::sync::{Mutex, Semaphore};
use tracing::info;

use super::{AddrMap, AgentClient, AgentPool, EnvMap, StorageMap};
use crate::{
    cli::Cli,
    db::Database,
    env::Environment,
    error::StateError,
    schema::storage::STORAGE_DIR,
    server::{error::StartError, prometheus::HttpsdResponse},
    util::OpaqueDebug,
};

/// The global state for the control plane.
#[derive(Debug)]
pub struct GlobalState {
    pub db: OpaqueDebug<Database>,
    pub cli: Cli,
    pub agent_key: Option<String>,
    pub pool: AgentPool,
    pub storage: StorageMap,
    pub envs: EnvMap,

    pub prom_httpsd: Mutex<HttpsdResponse>,
    pub prometheus: OpaqueDebug<Option<PrometheusClient>>,
}

impl GlobalState {
    pub async fn load(
        cli: Cli,
        db: Database,
        prometheus: Option<PrometheusClient>,
    ) -> Result<Arc<Self>, StartError> {
        // Load storage meta from persistence, then read the storage data from FS
        let storage_meta = db.storage.read_all();
        let storage = StorageMap::default();
        for ((network, id), meta) in storage_meta {
            let loaded = match meta.load(&cli).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("Error loading storage from persistence {network}/{id}: {e}");
                    continue;
                }
            };
            storage.insert((network, id), Arc::new(loaded));
        }

        let pool: DashMap<_, _> = db.agents.read_all().collect();

        let state = Arc::new(Self {
            cli,
            agent_key: std::env::var(ENV_AGENT_KEY).ok(),
            pool,
            storage,
            envs: EnvMap::default(),
            prom_httpsd: Default::default(),
            prometheus: OpaqueDebug(prometheus),
            db: OpaqueDebug(db),
        });

        let env_meta = state.db.envs.read_all().collect::<Vec<_>>();

        let num_cannons = env_meta.iter().map(|(_, e)| e.cannons.len()).sum();
        // this semaphor prevents cannons from starting until the environment is
        // created
        let cannons_ready = Arc::new(Semaphore::const_new(num_cannons));
        // when this guard is dropped, the semaphore is released
        let cannons_ready_guard = Arc::clone(&cannons_ready);
        let _cannons_guard = cannons_ready_guard
            .acquire_many(num_cannons as u32)
            .await
            .unwrap();

        for (id, meta) in env_meta.into_iter() {
            let loaded = match meta
                .load(Arc::clone(&state), Arc::clone(&cannons_ready))
                .await
            {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("Error loading storage from persistence {id}: {e}");
                    continue;
                }
            };
            info!("loaded env {id} from persistence");
            state.envs.insert(id, Arc::new(loaded));
        }

        // For all agents not in envs, set their state to Inventory
        for mut entry in state.pool.iter_mut() {
            let AgentState::Node(env, _) = entry.value().state() else {
                continue;
            };

            if state.envs.contains_key(env) {
                continue;
            }

            info!(
                "setting agent {} to Inventory state due to missing env {env}",
                entry.key()
            );
            entry.set_state(AgentState::Inventory);
            let _ = state.db.agents.save(entry.key(), entry.value());
        }

        Ok(state)
    }

    pub fn storage_path(&self, network: NetworkId, storage_id: StorageId) -> PathBuf {
        self.cli
            .path
            .join(STORAGE_DIR)
            .join(network.to_string())
            .join(storage_id.to_string())
    }

    /// Get a peer-to-addr mapping for a set of agents
    /// Locks pools for reading
    pub async fn get_addr_map(
        &self,
        filter: Option<&HashSet<AgentId>>,
    ) -> Result<AddrMap, StateError> {
        self.pool
            .iter()
            .filter(|agent| filter.is_none() || filter.is_some_and(|p| p.contains(&agent.id())))
            .map(|agent| {
                let addrs = agent
                    .addrs
                    .as_ref()
                    .ok_or_else(|| StateError::NoAddress(agent.id()))?;
                Ok((agent.id(), addrs.clone()))
            })
            .collect()
    }

    /// Lookup an rpc client by agent id.
    /// Locks pools for reading
    pub fn get_client(&self, id: AgentId) -> Option<AgentClient> {
        self.pool.get(&id)?.client_owned()
    }

    pub fn get_agent_rest(&self, id: AgentId) -> Option<SocketAddr> {
        let agent = self.pool.get(&id)?;
        Some(SocketAddr::new(agent.addrs()?.usable()?, agent.rest_port()))
    }

    pub fn get_env(&self, id: EnvId) -> Option<Arc<Environment>> {
        Some(Arc::clone(self.envs.get(&id)?.value()))
    }
}
