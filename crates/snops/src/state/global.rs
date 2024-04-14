use std::{collections::HashSet, sync::Arc};

use prometheus_http_query::Client as PrometheusClient;
use snops_common::state::AgentId;
use tokio::sync::{Mutex, RwLock};

use super::{persist::PersistStorage, AddrMap, AgentClient, AgentPool, EnvMap, StorageMap};
use crate::{
    cli::Cli,
    db::Database,
    env::persist::PersistEnv,
    error::StateError,
    server::{error::StartError, prometheus::HttpsdResponse},
    util::OpaqueDebug,
};

/// The global state for the control plane.
#[derive(Debug)]
pub struct GlobalState {
    pub db: Database,
    pub cli: Cli,
    pub pool: RwLock<AgentPool>,
    pub storage: RwLock<StorageMap>,
    pub envs: RwLock<EnvMap>,

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
        let mut storage = StorageMap::default();
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
        let mut envs = EnvMap::default();
        for meta in env_meta {
            let id = meta.id;
            let loaded = match meta.load(&db, &storage, &cli).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("Error loading storage from persistence {id}: {e}");
                    continue;
                }
            };
            envs.insert(id, Arc::new(loaded));
        }

        Ok(Self {
            cli,
            pool: RwLock::new(db.load()?),
            storage: RwLock::new(storage),
            envs: RwLock::new(envs),
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
            .read()
            .await
            .iter()
            .filter(|(id, _)| filter.is_none() || filter.is_some_and(|p| p.contains(id)))
            .map(|(id, agent)| {
                let addrs = agent
                    .addrs
                    .as_ref()
                    .ok_or_else(|| StateError::NoAddress(*id))?;
                Ok((*id, addrs.clone()))
            })
            .collect()
    }

    /// Lookup an rpc client by agent id.
    /// Locks pools for reading
    pub async fn get_client(&self, id: AgentId) -> Option<AgentClient> {
        self.pool
            .read()
            .await
            .get(&id)
            .and_then(|a| a.client_owned())
    }
}
