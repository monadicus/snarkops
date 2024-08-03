use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use crate::rpc::error::*;
use crate::{
    prelude::EnvId,
    state::{AgentState, NetworkId, PortConfig},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Handshake {
    pub jwt: Option<String>,
    pub loki: Option<String>,
    pub state: AgentState,
}

/// The RPC service that agents implement as a server.
#[tarpc::service]
pub trait AgentService {
    /// Handshake with some initial connection details.
    async fn handshake(handshake: Handshake) -> Result<(), ReconcileError>;

    /// Control plane asks the agent for its external network address, along
    /// with local addrs.
    async fn get_addrs() -> (PortConfig, Option<IpAddr>, Vec<IpAddr>);

    /// Control plane instructs the agent to reconcile towards a particular
    /// state.
    async fn reconcile(to: AgentState) -> Result<(), ReconcileError>;

    /// Broadcast a transaction locally
    async fn broadcast_tx(tx: String) -> Result<(), AgentError>;

    /// Make a GET request to the snarkos server
    async fn snarkos_get(route: String) -> Result<String, SnarkosRequestError>;

    /// Close the agent process
    async fn kill();

    /// Locally execute an authorization, using the given query
    /// environment id is passed so the agent can determine which aot binary to
    /// use
    async fn execute_authorization(
        env_id: EnvId,
        network: NetworkId,
        query: String,
        auth: String,
    ) -> Result<String, AgentError>;

    async fn get_metric(metric: AgentMetric) -> f64;

    async fn set_log_level(level: String) -> Result<(), AgentError>;

    async fn set_aot_log_level(verbosity: u8) -> Result<(), AgentError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentMetric {
    Tps,
}
