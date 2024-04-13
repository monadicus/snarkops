use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bimap::BiMap;
use prometheus_http_query::Client as PrometheusClient;
use snops_common::state::{AgentId, EnvId};
use tokio::sync::{Mutex, RwLock};

use super::{AddrMap, AgentClient, AgentPool};
use crate::{
    cli::Cli,
    db::Database,
    env::Environment,
    error::StateError,
    schema::storage::LoadedStorage,
    server::{error::StartError, prometheus::HttpsdResponse},
    util::OpaqueDebug,
};

/// The global state for the control plane.
#[derive(Debug)]
pub struct GlobalState {
    pub cli: Cli,
    pub pool: RwLock<AgentPool>,
    /// A map from ephemeral integer storage ID to actual storage ID.
    pub storage_ids: RwLock<BiMap<usize, String>>,
    pub storage: RwLock<HashMap<usize, Arc<LoadedStorage>>>,

    pub envs: RwLock<HashMap<EnvId, Arc<Environment>>>,

    pub prom_httpsd: Mutex<HttpsdResponse>,
    pub prometheus: OpaqueDebug<Option<PrometheusClient>>,
}

impl GlobalState {
    pub fn load(
        cli: Cli,
        db: Database,
        prometheus: Option<PrometheusClient>,
    ) -> Result<Self, StartError> {
        Ok(Self {
            cli,
            pool: Default::default(),
            storage_ids: Default::default(),
            storage: Default::default(),
            envs: Default::default(),
            prom_httpsd: Default::default(),
            prometheus: OpaqueDebug(prometheus),
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
