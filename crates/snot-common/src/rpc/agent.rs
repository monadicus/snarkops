use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::control::ResolveError;
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
    async fn get_metric(metric: AgentMetric) -> f64;
}

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum ReconcileError {
    #[error("aborted by a more recent reconcilation request")]
    Aborted,
    #[error("failed to download the specified storage")]
    StorageAcquireError,
    #[error("failed to resolve addresses of stated peers")]
    ResolveAddrError(ResolveError),
    #[error("unknown error")]
    Unknown,
}

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum AgentError {
    #[error("invalid agent state")]
    InvalidState,
    #[error("failed to parse json")]
    FailedToParseJson,
    #[error("failed to make a request")]
    FailedToMakeRequest,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentMetric {
    Tps,
}
