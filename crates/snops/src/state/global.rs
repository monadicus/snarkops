use std::{collections::HashSet, fmt::Display, net::SocketAddr, path::PathBuf, sync::Arc};

use dashmap::DashMap;
use prometheus_http_query::Client as PrometheusClient;
use rand::seq::SliceRandom;
use serde::de::DeserializeOwned;
use snops_common::{
    constant::ENV_AGENT_KEY,
    node_targets::NodeTargets,
    state::{AgentId, AgentPeer, AgentState, EnvId, NetworkId, StorageId},
};
use tokio::sync::{Mutex, Semaphore};
use tracing::info;

use super::{AddrMap, AgentClient, AgentPool, EnvMap, StorageMap};
use crate::{
    cli::Cli,
    db::Database,
    env::{error::EnvRequestError, Environment, PortType},
    error::StateError,
    schema::storage::STORAGE_DIR,
    server::{error::StartError, prometheus::HttpsdResponse},
    util::OpaqueDebug,
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

    pub fn get_agent_rest(&self, id: AgentId) -> Option<SocketAddr> {
        let agent = self.pool.get(&id)?;
        Some(SocketAddr::new(agent.addrs()?.usable()?, agent.rest_port()))
    }

    pub fn get_env(&self, id: EnvId) -> Option<Arc<Environment>> {
        Some(Arc::clone(self.envs.get(&id)?.value()))
    }

    pub async fn snarkos_get<T: DeserializeOwned>(
        &self,
        env_id: EnvId,
        route: impl Display,
        target: &NodeTargets,
    ) -> Result<T, EnvRequestError> {
        let Some(env) = self.get_env(env_id) else {
            return Err(EnvRequestError::MissingEnv(env_id));
        };

        let network = env.network;

        let mut query_nodes = env
            .matching_nodes(target, &self.pool, PortType::Rest)
            // collecting here is required to avoid a long lived borrow on the agent
            // pool if this collect is removed, the iterator
            // will not be Send, and axum will be sad
            .collect::<Vec<_>>();

        if query_nodes.is_empty() {
            return Err(EnvRequestError::NoMatchingNodes);
        }

        // select nodes in a random order
        query_nodes.shuffle(&mut rand::thread_rng());

        // walk through the nodes until we find one that responds
        for peer in query_nodes {
            let addr = match peer {
                AgentPeer::Internal(agent_id, _) => {
                    // ensure the node state is online
                    if !self.is_agent_node_online(agent_id) {
                        continue;
                    };

                    // attempt to get the state root from the client via RPC
                    if let Some(client) = self.get_client(agent_id) {
                        match client.snarkos_get::<T>(&route).await {
                            Ok(res) => return Ok(res),
                            Err(e) => {
                                tracing::error!(
                                    "env {env_id} agent {agent_id} request failed: {e}"
                                );
                                continue;
                            }
                        }
                    }

                    // get the agent's rest address as a fallback for the client
                    if let Some(sock_addr) = self.get_agent_rest(agent_id) {
                        sock_addr
                    } else {
                        continue;
                    }
                }
                AgentPeer::External(addr) => addr,
            };

            // attempt to get the state root from the internal or external node via REST
            let url = format!("http://{addr}/{network}{route}");
            match REST_CLIENT.get(&url).send().await {
                Ok(res) => match res.json().await {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::error!("env {env_id} peer {peer:?} failed to parse {url}: {e}");
                        continue;
                    }
                },
                Err(e) => {
                    tracing::error!("env {env_id} peer {peer:?} failed to make request {url}: {e}");
                    continue;
                }
            }
        }

        Err(EnvRequestError::NoResponsiveNodes)
    }
}
