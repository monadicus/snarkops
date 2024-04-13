use std::{
    collections::HashSet,
    net::IpAddr,
    sync::{Arc, Weak},
    time::Instant,
};

use fixedbitset::FixedBitSet;
use jwt::SignWithKey;
use serde::Deserialize;
use snops_common::{
    lasso::Spur,
    rpc::agent::AgentServiceClient,
    set::{MaskBit, MASK_PREFIX_LEN},
    state::{AgentId, AgentMode, AgentState, NodeState, PortConfig},
    INTERN,
};

use super::AgentClient;
use crate::server::jwt::{Claims, JWT_NONCE, JWT_SECRET};

#[derive(Debug)]
/// Apparently `const* ()` is not send, so this is a workaround
pub struct Busy;

/// An active agent, known by the control plane.
#[derive(Debug)]
pub struct Agent {
    id: AgentId,
    claims: Claims,
    connection: AgentConnection,
    state: AgentState,

    /// CLI provided information (mode, labels, local private key)
    flags: AgentFlags,

    /// Count of how many executions this agent is currently working on
    compute_claim: Arc<Busy>,
    /// Count of how many environments this agent is currently
    env_claim: Arc<Busy>,

    /// The external address of the agent, along with its local addresses.
    ports: Option<PortConfig>,
    pub(super) addrs: Option<AgentAddrs>,
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
#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct AgentFlags {
    #[serde(deserialize_with = "deser_mode")]
    mode: AgentMode,
    #[serde(deserialize_with = "deser_labels")]
    labels: HashSet<Spur>,
    #[serde(deserialize_with = "deser_pk", default)]
    local_pk: bool,
}

fn deser_mode<'de, D>(deser: D) -> Result<AgentMode, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // axum's querystring visitor marks all values as string
    let byte: u8 = <&str>::deserialize(deser)?
        .parse()
        .map_err(|e| serde::de::Error::custom(format!("error parsing u8: {e}")))?;
    Ok(AgentMode::from(byte))
}

pub fn deser_labels<'de, D>(deser: D) -> Result<HashSet<Spur>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deser)?
        .map(|s| {
            s.split(',')
                .filter(|s| !s.is_empty())
                .map(|s| INTERN.get_or_intern(s))
                .collect()
        })
        .unwrap_or_default())
}

pub fn deser_pk<'de, D>(deser: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // axum's querystring visitor marks all values as string
    Ok(Option::<&str>::deserialize(deser)?
        .map(|s| s == "true")
        .unwrap_or(false))
}

impl AgentFlags {
    pub fn mask(&self, labels: &[Spur]) -> FixedBitSet {
        let mut mask = FixedBitSet::with_capacity(labels.len() + MASK_PREFIX_LEN);
        if self.mode.validator {
            mask.insert(MaskBit::Validator as usize);
        }
        if self.mode.prover {
            mask.insert(MaskBit::Prover as usize);
        }
        if self.mode.client {
            mask.insert(MaskBit::Client as usize);
        }
        if self.mode.compute {
            mask.insert(MaskBit::Compute as usize);
        }
        if self.local_pk {
            mask.insert(MaskBit::LocalPrivateKey as usize);
        }

        for (i, label) in labels.iter().enumerate() {
            if self.labels.contains(label) {
                mask.insert(i + MASK_PREFIX_LEN);
            }
        }
        mask
    }
}
