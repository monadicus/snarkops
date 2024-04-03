use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use super::error::*;
use crate::state::{AgentState, PortConfig};

/// The RPC service that agents implement as a server.
#[tarpc::service]
pub trait AgentService {
    /// Control plane instructs the agent to use a JWT when reconnecting later.
    async fn keep_jwt(jwt: String);

    /// Control plane asks the agent for its external network address, along
    /// with local addrs.
    async fn get_addrs() -> (PortConfig, Option<IpAddr>, Vec<IpAddr>);

    /// Control plane instructs the agent to reconcile towards a particular
    /// state.
    async fn reconcile(to: AgentState) -> Result<(), ReconcileError>;

    /// Get the state root from the running node
    async fn get_state_root() -> Result<String, AgentError>;

    /// Broadcast a transaction locally
    async fn broadcast_tx(tx: String) -> Result<(), AgentError>;

    /// Locally execute an authorization, using the given query
    /// environment id is passed so the agent can determine which aot binary to
    /// use
    async fn execute_authorization(
        env_id: usize,
        query: String,
        auth: String,
    ) -> Result<(), AgentError>;

    async fn get_metric(metric: AgentMetric) -> f64;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentMetric {
    Tps,
}
