use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use crate::api::EnvInfo;
use crate::rpc::error::*;
use crate::state::snarkos_status::SnarkOSLiteBlock;
use crate::{
    prelude::EnvId,
    state::{AgentState, NetworkId, PortConfig},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Handshake {
    pub jwt: Option<String>,
    pub loki: Option<String>,
    pub state: AgentState,
    pub env_info: Option<(EnvId, EnvInfo)>,
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
    async fn set_agent_state(
        to: AgentState,
        env_info: Option<(EnvId, EnvInfo)>,
    ) -> Result<(), ReconcileError>;

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

    /// Find a transaction's block hash by its transaction id
    async fn find_transaction(tx_id: String) -> Result<Option<String>, AgentError>;

    /// Get a block info and transaction data from the agent's running node
    async fn get_snarkos_block_lite(
        block_hash: String,
    ) -> Result<Option<SnarkOSLiteBlock>, AgentError>;

    async fn set_aot_log_level(verbosity: u8) -> Result<(), AgentError>;

    async fn get_status() -> Result<AgentStatus, AgentError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub aot_online: bool,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentMetric {
    Tps,
}
