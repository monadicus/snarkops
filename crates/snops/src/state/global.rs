use std::{collections::HashSet, fmt::Display, net::SocketAddr, path::PathBuf, sync::Arc};

use chrono::Utc;
use dashmap::DashMap;
use lazysort::SortedBy;
use prometheus_http_query::Client as PrometheusClient;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use snops_common::{
    constant::ENV_AGENT_KEY,
    node_targets::NodeTargets,
    rpc::error::SnarkosRequestError,
    state::{AgentId, AgentPeer, AgentState, EnvId, LatestBlockInfo, NetworkId, StorageId},
    util::OpaqueDebug,
};
use tokio::sync::{Mutex, Semaphore};
use tracing::info;

use super::{AddrMap, AgentClient, AgentPool, EnvMap, StorageMap};
use crate::{
    cli::Cli,
    db::Database,
    env::{error::EnvRequestError, Environment, PortType},
    error::StateError,
    schema::storage::{LoadedStorage, STORAGE_DIR},
    server::{error::StartError, prometheus::HttpsdResponse},
};

lazy_static::lazy_static! {
    pub(crate) static ref REST_CLIENT: reqwest::Client = reqwest::Client::new();
}

/// The global state for the control plane.
#[derive(Debug)]
pub struct GlobalState {
    pub db: OpaqueDebug<Database>,
    pub cli: Cli,
    pub agent_key: Option<String>,
    pub pool: AgentPool,
    pub storage: StorageMap,
    pub envs: EnvMap,
    pub env_block_info: DashMap<EnvId, LatestBlockInfo>,

    pub prom_httpsd: Mutex<HttpsdResponse>,
    pub prometheus: OpaqueDebug<Option<PrometheusClient>>,
}

/// A ranked peer item, with a score reflecting the freshness of the block info
///
/// (Score, BlockInfo, AgentId, SocketAddr)
///
/// Also contains a socket address in case the peer is external (or the agent is
/// not responding)
///
/// To be used with a lazy sorted iterator to get the best peer
type RankedPeerItem = (
    u32,
    Option<LatestBlockInfo>,
    Option<AgentId>,
    Option<SocketAddr>,
);

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
            env_block_info: Default::default(),
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

    /// check if an agent's node is in an online state
    pub fn is_agent_node_online(&self, id: AgentId) -> bool {
        let Some(agent) = self.pool.get(&id) else {
            return false;
        };

        match agent.state() {
            AgentState::Node(_, state) => state.online,
            _ => false,
        }
    }

    pub fn try_unload_storage(
        &self,
        network: NetworkId,
        id: StorageId,
    ) -> Option<Arc<LoadedStorage>> {
        // if the storage is in use, don't unload it
        if self
            .envs
            .iter()
            .any(|e| e.storage.id == id && e.storage.network == network)
        {
            return None;
        }

        let (_, storage) = self.storage.remove(&(network, id))?;
        if let Err(e) = self.db.storage.delete(&(network, id)) {
            tracing::error!("[storage {network}.{id}] failed to delete persistence: {e}");
        }
        Some(storage)
    }

    pub fn get_env(&self, id: EnvId) -> Option<Arc<Environment>> {
        Some(Arc::clone(self.envs.get(&id)?.value()))
    }

    pub fn get_env_block_info(&self, id: EnvId) -> Option<LatestBlockInfo> {
        self.env_block_info.get(&id).map(|info| info.clone())
    }

    pub fn update_env_block_info(&self, id: EnvId, info: &LatestBlockInfo) {
        use dashmap::mapref::entry::Entry::*;
        match self.env_block_info.entry(id) {
            Occupied(ent) if ent.get().block_timestamp < info.block_timestamp => {
                ent.replace_entry(info.clone());
            }
            Vacant(ent) => {
                ent.insert(info.clone());
            }
            _ => {}
        }
    }

    /// Get a vec of peers and their addresses, along with a score reflecting
    /// the freshness of the block info
    pub fn get_scored_peers(&self, env_id: EnvId, target: &NodeTargets) -> Vec<RankedPeerItem> {
        let Some(env) = self.get_env(env_id) else {
            return Vec::new();
        };

        let now = Utc::now();

        env.matching_nodes(target, &self.pool, PortType::Rest)
            .filter_map(|peer| {
                let agent_id = match peer {
                    AgentPeer::Internal(id, _) => id,
                    // TODO: periodically get block info from external nodes
                    AgentPeer::External(addr) => return Some((0u32, None, None, Some(addr))),
                };

                let agent = self.pool.get(&agent_id)?;

                // ensure the node state is online
                if !matches!(agent.state(), AgentState::Node(_, _)) {
                    return None;
                }

                Some((
                    agent
                        .status
                        .block_info
                        .as_ref()
                        .map(|info| info.score(&now))
                        .unwrap_or_default(),
                    agent.status.block_info.clone(),
                    Some(agent_id),
                    agent.rest_addr(),
                ))
            })
            .collect()
    }

    pub async fn snarkos_get<T: DeserializeOwned + Clone>(
        &self,
        env_id: EnvId,
        route: impl Display,
        target: &NodeTargets,
    ) -> Result<T, EnvRequestError> {
        let Some(env) = self.get_env(env_id) else {
            return Err(EnvRequestError::MissingEnv(env_id));
        };

        let network = env.network;

        let query_nodes = self.get_scored_peers(env_id, target);
        if query_nodes.is_empty() {
            return Err(EnvRequestError::NoMatchingNodes);
        }

        let route_string = route.to_string();
        let route_str = route_string.as_ref();
        let is_state_root = matches!(route_str, "/latest/stateRoot" | "/stateRoot/latest");
        let is_block_height = matches!(route_str, "/latest/height" | "/block/height/latest");
        let is_block_hash = matches!(route_str, "/latest/hash" | "/block/hash/latest");

        /// I would rather reparse a string than use unsafe/dyn any here
        // because we would be making a request anyway and it's not a big deal.
        fn json_generics_bodge<T: DeserializeOwned>(
            v: impl Serialize,
        ) -> Result<T, EnvRequestError> {
            serde_json::from_value(json!(&v)).map_err(|e| {
                EnvRequestError::AgentRequestError(SnarkosRequestError::JsonParseError(
                    e.to_string(),
                ))
            })
        }

        // walk through the nodes (lazily sorted by a score) until we find one that
        // responds
        for (_, info, agent_id, addr) in query_nodes.into_iter().sorted_by(|a, b| a.0.cmp(&b.0)) {
            // if this route is a route with block info that we already track,
            // we can return the info from the agent's status directly
            if let Some(info) = info {
                if is_state_root {
                    return json_generics_bodge(info.state_root);
                } else if is_block_height {
                    return json_generics_bodge(info.height);
                } else if is_block_hash {
                    return json_generics_bodge(info.block_hash);
                };
            }

            // attempt to make a request through the client via RPC if this is an agent
            if let Some(agent_id) = agent_id {
                if let Some(client) = self.get_client(agent_id) {
                    match client.snarkos_get::<T>(&route).await {
                        Ok(res) => return Ok(res),
                        Err(e) => {
                            tracing::error!("env {env_id} agent {agent_id} request failed: {e}");
                            continue;
                        }
                    }
                }
            }

            // if we have an address, we can try to make a request via REST
            let Some(addr) = addr else {
                continue;
            };

            // attempt to make the request from the node via REST
            let url = format!("http://{addr}/{network}{route}");
            let Ok(res) = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                REST_CLIENT.get(&url).send(),
            )
            .await
            else {
                // timeout
                continue;
            };
            match res {
                Ok(res) => match res.json::<T>().await {
                    Ok(e) => return Ok(e),
                    Err(e) => {
                        tracing::error!(
                            "env {env_id} request {addr:?} failed to parse {url}: {e:?}"
                        );
                        continue;
                    }
                },
                Err(e) => {
                    tracing::error!(
                        "env {env_id} request {addr:?} failed to make request {url}: {e:?}"
                    );
                    continue;
                }
            }
        }

        Err(EnvRequestError::NoResponsiveNodes)
    }
}
