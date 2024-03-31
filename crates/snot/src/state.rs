use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    sync::{Arc, Weak},
    time::Instant,
};

use anyhow::{anyhow, Result};
use bimap::BiMap;
use fixedbitset::FixedBitSet;
use jwt::SignWithKey;
use snot_common::{
    lasso::Spur,
    rpc::agent::{AgentServiceClient, ReconcileError},
    state::{AgentId, AgentMode, AgentState, NodeState, NodeType, PortConfig},
    INTERN,
};
use surrealdb::{engine::local::Db, Surreal};
use tarpc::{client::RpcError, context};
use tokio::sync::RwLock;

use crate::{
    cli::Cli,
    env::Environment,
    schema::storage::LoadedStorage,
    server::jwt::{Claims, JWT_NONCE, JWT_SECRET},
};

pub type AppState = Arc<GlobalState>;

/// The global state for the control plane.
#[derive(Debug)]
pub struct GlobalState {
    pub cli: Cli,
    pub db: Surreal<Db>,
    pub pool: RwLock<HashMap<AgentId, Agent>>,
    /// A map from ephemeral integer storage ID to actual storage ID.
    pub storage_ids: RwLock<BiMap<usize, String>>,
    pub storage: RwLock<HashMap<usize, Arc<LoadedStorage>>>,

    pub envs: RwLock<HashMap<usize, Arc<Environment>>>,
}

/// This is the representation of a public addr or a list of internal addrs.
pub type AgentAddrs = (Option<IpAddr>, Vec<IpAddr>);

/// An active agent, known by the control plane.
#[derive(Debug)]
pub struct Agent {
    id: AgentId,
    claims: Claims,
    connection: AgentConnection,
    state: AgentState,

    /// CLI provided labels for this agent
    labels: HashSet<Spur>,
    /// Available modes for this agent
    mode: AgentMode,

    /// Count of how many executions this agent is currently working on
    compute_claim: Arc<Busy>,
    /// Count of how many environments this agent is currently
    env_claim: Arc<Busy>,

    /// The external address of the agent, along with its local addresses.
    ports: Option<PortConfig>,
    addrs: Option<AgentAddrs>,
}

#[derive(Debug)]
/// Apparently `const* ()` is not send, so this is a workaround
pub struct Busy;

pub struct AgentClient(AgentServiceClient);

impl Agent {
    pub fn new(rpc: AgentServiceClient, id: AgentId, mode: AgentMode, labels: Vec<String>) -> Self {
        Self {
            id,
            mode,
            labels: labels
                .into_iter()
                .map(|s| INTERN.get_or_intern(s))
                .collect(),
            compute_claim: Arc::new(Busy),
            env_claim: Arc::new(Busy),
            claims: Claims {
                id,
                nonce: *JWT_NONCE,
            },
            connection: AgentConnection::Online(rpc),
            state: Default::default(),
            ports: None,
            addrs: None,
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.connection, AgentConnection::Online(_))
    }

    /// Whether this agent is capable of being a node in the network.
    pub fn is_node_capable(&self) -> bool {
        if !self.is_connected() || self.addrs.is_none() {
            return false;
        };
        let (external, internal) = self.addrs.as_ref().unwrap();
        external.is_some() || !internal.is_empty()
    }

    /// Check if an agent has a set of labels
    pub fn has_labels(&self, labels: &HashSet<Spur>) -> bool {
        labels.is_empty() || self.labels.intersection(labels).count() == labels.len()
    }

    /// Check if an agent has a specific label
    pub fn has_label(&self, label: &str) -> bool {
        INTERN
            .get(label)
            .map_or(false, |label| self.labels.contains(&label))
    }

    pub fn str_labels(&self) -> HashSet<&str> {
        self.labels.iter().map(|s| INTERN.resolve(s)).collect()
    }

    // Get the mask of this agent
    pub fn mask(&self, labels: &[Spur]) -> FixedBitSet {
        let mut mask = FixedBitSet::with_capacity(labels.len() + 4);
        if self.mode.validator {
            mask.insert(NodeType::Validator.bit());
        }
        if self.mode.prover {
            mask.insert(NodeType::Prover.bit());
        }
        if self.mode.client {
            mask.insert(NodeType::Client.bit());
        }
        if self.mode.compute {
            mask.insert(3);
        }

        for (i, label) in labels.iter().enumerate() {
            if self.labels.contains(label) {
                mask.insert(i + 4);
            }
        }
        mask
    }

    /// Check if an agent is in inventory state
    pub fn is_inventory(&self) -> bool {
        matches!(self.state, AgentState::Inventory)
    }

    /// Check if an agent is available for compute tasks
    pub fn can_compute(&self) -> bool {
        self.is_inventory() && self.mode.compute && !self.is_compute_claimed()
    }

    /// Check if an agent is working on an authorization
    pub fn is_compute_claimed(&self) -> bool {
        Arc::strong_count(&self.compute_claim) > 1
    }

    /// Mark an agent as busy. This is used to prevent multiple authorizations
    pub fn make_busy(&self) -> Arc<Busy> {
        Arc::clone(&self.compute_claim)
    }

    /// Mark an agent as busy. This is used to prevent multiple authorizations
    pub fn get_compute_claim(&self) -> Weak<Busy> {
        Arc::downgrade(&self.compute_claim)
    }

    /// Check if an agent is owned by an environment
    pub fn is_env_claimed(&self) -> bool {
        Arc::strong_count(&self.env_claim) > 1
    }

    /// Get a weak reference to the env claim, which can be used to later lock this
    /// agent for an environment.
    pub fn get_env_claim(&self) -> Weak<Busy> {
        Arc::downgrade(&self.env_claim)
    }

    /// The ID of this agent.
    pub fn id(&self) -> AgentId {
        self.id
    }

    /// The current state of this agent.
    pub fn state(&self) -> &AgentState {
        &self.state
    }

    pub fn modes(&self) -> AgentMode {
        self.mode
    }

    pub fn claims(&self) -> &Claims {
        &self.claims
    }

    pub fn sign_jwt(&self) -> String {
        self.claims.to_owned().sign_with_key(&*JWT_SECRET).unwrap()
    }

    pub fn rpc(&self) -> Option<&AgentServiceClient> {
        match self.connection {
            AgentConnection::Online(ref rpc) => Some(rpc),
            _ => None,
        }
    }

    /// Get an owned copy of the RPC client for making reconcilation calls.
    /// `None` if the client is not currently connected.
    pub fn client_owned(&self) -> Option<AgentClient> {
        match self.connection {
            AgentConnection::Online(ref rpc) => Some(AgentClient(rpc.to_owned())),
            _ => None,
        }
    }

    /// Forcibly remove the RPC connection to this client. Called when an agent
    /// disconnects.
    pub fn mark_disconnected(&mut self) {
        self.connection = AgentConnection::Offline {
            since: Instant::now(),
        };
    }

    pub fn mark_connected(&mut self, client: AgentServiceClient) {
        self.connection = AgentConnection::Online(client);
    }

    /// Forcibly sets an agent's state. This does **not** reconcile the agent,
    /// and should only be called after an agent is reconciled.
    pub fn set_state(&mut self, state: AgentState) {
        self.state = state;
    }

    /// Set the ports of the agent. This does **not** trigger a reconcile
    pub fn set_ports(&mut self, ports: PortConfig) {
        self.ports = Some(ports);
    }

    // Gets the bft port of the agent. Assumes the agent is ready, returns 0 if not.
    pub fn bft_port(&self) -> u16 {
        self.ports.as_ref().map(|p| p.bft).unwrap_or_default()
    }

    // Gets the node port of the agent. Assumes the agent is ready, returns 0 if
    // not.
    pub fn node_port(&self) -> u16 {
        self.ports.as_ref().map(|p| p.node).unwrap_or_default()
    }

    // Gets the rest port of the agent. Assumes the agent is ready, returns 0 if
    // not.
    pub fn rest_port(&self) -> u16 {
        self.ports.as_ref().map(|p| p.rest).unwrap_or_default()
    }

    /// Set the external and internal addresses of the agent. This does **not**
    /// trigger a reconcile
    pub fn set_addrs(&mut self, external_addr: Option<IpAddr>, internal_addrs: Vec<IpAddr>) {
        self.addrs = Some((external_addr, internal_addrs));
    }

    pub fn map_to_node_state_reconcile<F>(&self, f: F) -> Option<(AgentId, AgentClient, AgentState)>
    where
        F: Fn(NodeState) -> NodeState,
    {
        Some((
            self.id(),
            self.client_owned()?,
            match &self.state {
                AgentState::Node(id, state) => AgentState::Node(*id, f(state.clone())),
                _ => return None,
            },
        ))
    }
}

impl AgentClient {
    pub async fn reconcile(
        &self,
        to: AgentState,
    ) -> Result<Result<AgentState, ReconcileError>, RpcError> {
        self.0
            .reconcile(context::current(), to.clone())
            .await
            .map(|res| res.map(|_| to))
    }

    pub async fn get_state_root(&self) -> Result<String> {
        Ok(self.0.get_state_root(context::current()).await??)
    }

    pub async fn execute_authorization(
        &self,
        env_id: usize,
        query: String,
        auth: String,
    ) -> Result<()> {
        self.0
            .execute_authorization(context::current(), env_id, query, auth)
            .await?
            .map_err(anyhow::Error::from)
    }

    pub async fn broadcast_tx(&self, tx: String) -> Result<()> {
        self.0
            .broadcast_tx(context::current(), tx)
            .await?
            .map_err(anyhow::Error::from)
    }
}

#[derive(Debug, Clone)]
pub enum AgentConnection {
    Online(AgentServiceClient),
    Offline { since: Instant },
}

pub type AddrMap = HashMap<AgentId, AgentAddrs>;

/// Given a map of addresses, resolve the addresses of a set of peers relative
/// to a source agent.
pub fn resolve_addrs(
    addr_map: &AddrMap,
    src: AgentId,
    peers: &HashSet<AgentId>,
) -> Result<HashMap<AgentId, IpAddr>> {
    let src_addrs = addr_map
        .get(&src)
        .ok_or_else(|| anyhow!("source agent not found"))?;

    let all_internal = addr_map.values().all(|(ext, _)| ext.is_none());

    Ok(peers
        .iter()
        .filter_map(|id| {
            // ignore the source agent
            if *id == src {
                return None;
            }

            // if the agent has no addresses, skip it
            let addrs = addr_map.get(id)?;

            // if there are no external addresses in the entire addr map,
            // use the first internal address
            if all_internal {
                return addrs.1.first().copied().map(|addr| (*id, addr));
            }

            match (src_addrs.0, addrs.0, addrs.1.first()) {
                // if peers have the same external address, use the first internal address
                (Some(src_ext), Some(peer_ext), Some(peer_int)) if src_ext == peer_ext => {
                    Some((*id, *peer_int))
                }
                // otherwise use the external address
                (_, Some(peer_ext), _) => Some((*id, peer_ext)),
                _ => None,
            }
        })
        .collect())
}
impl GlobalState {
    /// Get a peer-to-addr mapping for a set of agents
    /// Locks pools for reading
    pub async fn get_addr_map(&self, filter: Option<&HashSet<AgentId>>) -> Result<AddrMap> {
        self.pool
            .read()
            .await
            .iter()
            .filter(|(id, _)| filter.is_none() || filter.is_some_and(|p| p.contains(id)))
            .map(|(id, agent)| {
                let addrs = agent
                    .addrs
                    .as_ref()
                    .ok_or_else(|| anyhow!("agent has no addresses"))?;
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
