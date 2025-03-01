use std::{fmt::Display, net::SocketAddr, path::PathBuf, sync::Arc};

use chrono::Utc;
use dashmap::DashMap;
use lazysort::SortedBy;
use prometheus_http_query::Client as PrometheusClient;
use serde::de::DeserializeOwned;
use snops_common::{
    constant::ENV_AGENT_KEY,
    events::Event,
    node_targets::NodeTargets,
    schema::storage::STORAGE_DIR,
    state::{
        AgentId, AgentPeer, AgentState, EnvId, LatestBlockInfo, NetworkId, NodeType, StorageId,
    },
    util::OpaqueDebug,
};
use tokio::sync::Semaphore;
use tracing::info;

use super::{
    snarkos_request::{self, reparse_json_env},
    AddrMap, AgentClient, AgentPool, EnvMap, StorageMap,
};
use crate::{
    apply::LoadedStorage,
    cli::Cli,
    db::Database,
    env::{cache::NetworkCache, error::EnvRequestError, Environment, PortType},
    error::StateError,
    events::Events,
    server::error::StartError,
    ReloadHandler,
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
    pub env_network_cache: OpaqueDebug<DashMap<EnvId, NetworkCache>>,
    pub events: Events,

    pub prometheus: OpaqueDebug<Option<PrometheusClient>>,

    pub log_level_handler: ReloadHandler,
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
        log_level_handler: ReloadHandler,
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
            events: Default::default(),
            prometheus: OpaqueDebug(prometheus),
            db: OpaqueDebug(db),
            env_network_cache: Default::default(),
            log_level_handler,
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
            state.insert_env(id, Arc::new(loaded));
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
    pub async fn get_addr_map(&self, filter: &[AgentId]) -> Result<AddrMap, StateError> {
        filter
            .iter()
            .filter_map(|id| self.pool.get(id))
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

    pub fn insert_env(&self, env_id: EnvId, env: Arc<Environment>) {
        self.envs.insert(env_id, env);
        self.env_network_cache.insert(env_id, Default::default());
    }

    pub fn remove_env(&self, env_id: EnvId) -> Option<Arc<Environment>> {
        self.env_network_cache.remove(&env_id);
        self.envs.remove(&env_id).map(|(_, env)| env)
    }

    pub fn get_env(&self, id: EnvId) -> Option<Arc<Environment>> {
        Some(Arc::clone(self.envs.get(&id)?.value()))
    }

    pub fn get_env_block_info(&self, id: EnvId) -> Option<LatestBlockInfo> {
        self.env_network_cache
            .get(&id)
            .and_then(|cache| cache.latest.clone())
    }

    pub fn update_env_block_info(&self, id: EnvId, info: &LatestBlockInfo) -> bool {
        let mut cache = self.env_network_cache.entry(id).or_default();
        cache.update_latest_info(info)
    }

    /// Get a vec of peers and their addresses, along with a score reflecting
    /// the freshness of the block info
    pub fn get_scored_peers(&self, env_id: EnvId, target: &NodeTargets) -> Vec<RankedPeerItem> {
        let Some(env) = self.get_env(env_id) else {
            return Vec::new();
        };

        // use the network cache to lookup external peer info
        let cache = self.env_network_cache.get(&env_id);
        let ext_infos = cache.as_ref().map(|c| &c.external_peer_infos);

        let now = Utc::now();

        env.matching_peers(target, &self.pool, PortType::Rest)
            .filter_map(|(key, peer)| {
                // ignore prover nodes
                if key.ty == NodeType::Prover {
                    return None;
                }

                let agent_id = match peer {
                    AgentPeer::Internal(id, _) => id,
                    AgentPeer::External(addr) => {
                        // lookup the external peer info from the cache
                        return Some(if let Some(info) = ext_infos.and_then(|c| c.get(&key)) {
                            (info.score(&now), Some(info.clone()), None, None)
                        } else {
                            (0u32, None, None, Some(addr))
                        });
                    }
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

        let query_nodes = self.get_scored_peers(env_id, target);
        if query_nodes.is_empty() {
            return Err(EnvRequestError::NoMatchingNodes);
        }

        let route_str = route.to_string();
        let prefix = snarkos_request::route_prefix_check(&route_str);

        // walk through the nodes (lazily sorted by a score) until we find one that
        // responds
        for (_, info, agent_id, addr) in query_nodes.into_iter().sorted_by(|a, b| a.0.cmp(&b.0)) {
            // if this route is a route with block info that we already track,
            // we can return the info from the agent's status directly
            if let (Some(prefix), Some(info)) = (prefix, info) {
                use snarkos_request::RoutePrefix::*;
                return match prefix {
                    StateRoot => reparse_json_env(info.state_root),
                    BlockHeight => reparse_json_env(info.height),
                    BlockHash => reparse_json_env(info.block_hash),
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
            match snarkos_request::get_on_addr(env.network, &route_str, addr).await {
                Ok(res) => return Ok(res),
                Err(e) => {
                    tracing::error!("env {env_id} request to `{addr}{route_str}`: {e}");
                    continue;
                }
            }
        }

        Err(EnvRequestError::NoResponsiveNodes)
    }
}

pub trait GetGlobalState<'a> {
    /// Returns the global state.
    fn global_state(self) -> &'a GlobalState;
}

impl<'a> GetGlobalState<'a> for &'a GlobalState {
    fn global_state(self) -> &'a GlobalState {
        self
    }
}

impl<'a> GetGlobalState<'a> for &'a Arc<GlobalState> {
    fn global_state(self) -> &'a GlobalState {
        self
    }
}

pub trait EmitEvent {
    fn emit<'a>(self, state: impl GetGlobalState<'a>);
}

impl EmitEvent for Event {
    #[inline]
    fn emit<'a>(self, state: impl GetGlobalState<'a>) {
        state.global_state().events.emit(self);
    }
}
