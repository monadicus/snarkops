use std::{
    collections::HashSet,
    net::IpAddr,
    sync::{Arc, Weak},
    time::Instant,
};

use fixedbitset::FixedBitSet;
use jwt::SignWithKey;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaChaRng;
use serde::{Deserialize, Serialize};
use snops_common::{
    lasso::Spur,
    rpc::agent::AgentServiceClient,
    state::{AgentId, AgentMode, AgentState, EnvId, NodeState, PortConfig},
    INTERN,
};

use super::{AgentClient, AgentFlags};
use crate::server::jwt::{Claims, JWT_SECRET};

#[derive(Debug)]
/// Apparently `const* ()` is not send, so this is a workaround
pub struct Busy;

/// An active agent, known by the control plane.
#[derive(Debug)]
pub struct Agent {
    pub(crate) id: AgentId,
    pub(crate) claims: Claims,
    pub(crate) connection: AgentConnection,
    pub(crate) state: AgentState,

    /// CLI provided information (mode, labels, local private key)
    pub(crate) flags: AgentFlags,

    /// Count of how many executions this agent is currently working on
    pub(crate) compute_claim: Arc<Busy>,
    /// Count of how many environments this agent is pending for
    pub(crate) env_claim: Arc<Busy>,

    /// The external address of the agent, along with its local addresses.
    pub(crate) ports: Option<PortConfig>,
    pub(crate) addrs: Option<AgentAddrs>,
}

impl Agent {
    pub fn new(rpc: AgentServiceClient, id: AgentId, flags: AgentFlags) -> Self {
        Self {
            id,
            flags,
            compute_claim: Arc::new(Busy),
            env_claim: Arc::new(Busy),
            claims: Claims {
                id,
                nonce: ChaChaRng::from_entropy().gen(),
            },
            connection: AgentConnection::Online(rpc),
            state: Default::default(),
            ports: None,
            addrs: None,
        }
    }

    pub(crate) fn from_components(
        claims: Claims,
        state: AgentState,
        flags: AgentFlags,
        ports: Option<PortConfig>,
        addrs: Option<AgentAddrs>,
    ) -> Self {
        Self {
            id: claims.id,
            flags,
            compute_claim: Arc::new(Busy),
            env_claim: Arc::new(Busy),
            claims,
            connection: AgentConnection::Offline {
                since: Instant::now(),
            },
            state,
            ports,
            addrs,
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.connection, AgentConnection::Online(_))
    }

    /// Whether this agent is capable of being a node in the network.
    pub fn is_node_capable(&self) -> bool {
        if !self.is_connected() {
            return false;
        };

        self.addrs
            .as_ref()
            .map(AgentAddrs::is_some)
            .unwrap_or_default()
    }

    /// Check if an agent has a set of labels
    pub fn has_labels(&self, labels: &HashSet<Spur>) -> bool {
        labels.is_empty() || self.flags.labels.intersection(labels).count() == labels.len()
    }

    /// Check if an agent has a specific label
    pub fn has_label(&self, label: Spur) -> bool {
        self.flags.labels.contains(&label)
    }

    /// Check if an agent has a specific label
    pub fn has_label_str(&self, label: &str) -> bool {
        INTERN
            .get(label)
            .map_or(false, |label| self.flags.labels.contains(&label))
    }

    pub fn str_labels(&self) -> HashSet<&str> {
        self.flags
            .labels
            .iter()
            .map(|s| INTERN.resolve(s))
            .collect()
    }

    // Get the mask of this agent
    pub fn mask(&self, labels: &[Spur]) -> FixedBitSet {
        self.flags.mask(labels)
    }

    /// Check if an agent is in inventory state
    pub fn is_inventory(&self) -> bool {
        matches!(self.state, AgentState::Inventory)
    }

    /// Check if an agent is available for compute tasks
    pub fn can_compute(&self) -> bool {
        self.is_inventory() && self.flags.mode.compute && !self.is_compute_claimed()
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

    /// Get a weak reference to the env claim, which can be used to later lock
    /// this agent for an environment.
    pub fn get_env_claim(&self) -> Weak<Busy> {
        Arc::downgrade(&self.env_claim)
    }

    pub fn env(&self) -> Option<EnvId> {
        match &self.state {
            AgentState::Node(id, _) => Some(*id),
            _ => None,
        }
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
        self.flags.mode
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

    pub fn mark_connected(&mut self, client: AgentServiceClient, flags: AgentFlags) {
        self.connection = AgentConnection::Online(client);
        self.flags = flags;
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

    /// Gets the metrics port of the agent. Assumes the agent is ready, returns
    /// 0 if not.
    pub fn metrics_port(&self) -> u16 {
        self.ports.as_ref().map(|p| p.metrics).unwrap_or_default()
    }

    /// True when the agent is configured to provide its own local private key
    pub fn has_local_pk(&self) -> bool {
        self.flags.local_pk
    }

    pub fn addrs(&self) -> Option<&AgentAddrs> {
        self.addrs.as_ref()
    }

    /// Set the external and internal addresses of the agent. This does **not**
    /// trigger a reconcile
    pub fn set_addrs(&mut self, external: Option<IpAddr>, internal: Vec<IpAddr>) {
        self.addrs = Some(AgentAddrs { external, internal });
    }

    pub fn map_to_node_state_reconcile<F>(&self, f: F) -> Option<(AgentId, AgentClient, AgentState)>
    where
        F: Fn(NodeState) -> NodeState,
    {
        Some((
            self.id(),
            self.client_owned()?,
            match &self.state {
                AgentState::Node(id, state) => AgentState::Node(*id, Box::new(f(*state.clone()))),
                _ => return None,
            },
        ))
    }
}

#[derive(Debug, Clone)]
pub enum AgentConnection {
    Online(AgentServiceClient),
    Offline { since: Instant },
}

/// This is the representation of a public addr or a list of internal addrs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAddrs {
    pub external: Option<IpAddr>,
    pub internal: Vec<IpAddr>,
}

impl AgentAddrs {
    pub fn usable(&self) -> Option<IpAddr> {
        self.external
            .as_ref()
            .or_else(|| self.internal.first())
            .copied()
    }

    pub fn is_some(&self) -> bool {
        self.external.is_some() || !self.internal.is_empty()
    }
}
