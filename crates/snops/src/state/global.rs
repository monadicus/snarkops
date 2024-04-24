use std::{collections::HashSet, sync::Arc};

use prometheus_http_query::Client as PrometheusClient;
use snops_common::{
    constant::ENV_AGENT_KEY,
    state::{AgentId, AgentState, EnvId},
};
use tokio::sync::Mutex;
use tracing::info;

use super::{persist::PersistStorage, AddrMap, AgentClient, AgentPool, EnvMap, StorageMap};
use crate::{
    cli::Cli,
    db::{document::DbDocument, Database},
    env::{persist::PersistEnv, Environment},
    error::StateError,
    server::{error::StartError, prometheus::HttpsdResponse},
    util::OpaqueDebug,
};

/// The global state for the control plane.
#[derive(Debug)]
pub struct GlobalState {
    pub db: Database,
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
    ) -> Result<Self, StartError> {
        // Load storage meta from persistence, then read the storage data from FS
        let storage_meta = db.load::<Vec<PersistStorage>>()?;
        let storage = StorageMap::default();
        for meta in storage_meta {
            let id = meta.id;
            let loaded = match meta.load(&cli).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("Error loading storage from persistence {id}: {e}");
                    continue;
                }
            };
            storage.insert(id, Arc::new(loaded));
        }

        let env_meta = db.load::<Vec<PersistEnv>>()?;
        let envs = EnvMap::default();
        for meta in env_meta {
            let id = meta.id;
            let loaded = match meta.load(&db, &storage, &cli).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("Error loading storage from persistence {id}: {e}");
                    continue;
                }
            };
            info!("loaded env {id} from persistence");
            envs.insert(id, Arc::new(loaded));
        }

        let pool = db.load::<AgentPool>()?;

        // For all agents not in envs, set their state to Inventory
        for mut entry in pool.iter_mut() {
            let AgentState::Node(env, _) = entry.value().state() else {
                continue;
            };

            if envs.contains_key(env) {
                continue;
            }

            info!(
                "setting agent {} to Inventory state due to missing env",
                entry.key()
            );
            entry.set_state(AgentState::Inventory);
            let _ = entry.value().save(&db, *entry.key());
        }

        Ok(Self {
            cli,
            agent_key: std::env::var(ENV_AGENT_KEY).ok(),
            pool,
            storage,
            envs,
            prom_httpsd: Default::default(),
            prometheus: OpaqueDebug(prometheus),
            db,
        })
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
        self.pool.get(&id).and_then(|a| a.client_owned())
    }

    pub fn get_env(&self, id: EnvId) -> Option<Arc<Environment>> {
        Some(Arc::clone(self.envs.get(&id)?.value()))
    }
}
