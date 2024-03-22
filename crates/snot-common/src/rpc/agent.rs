use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::state::AgentState;

/// The RPC service that agents implement as a server.
#[tarpc::service]
pub trait AgentService {
    /// Control plane instructs the agent to use a JWT when reconnecting later.
    async fn keep_jwt(jwt: String);

    /// Control plane asks the agent for its external network address, along with local addrs.
    async fn get_addrs() -> (Option<IpAddr>, Vec<IpAddr>);

    /// Control plane instructs the agent to reconcile towards a particular
    /// state.
    async fn reconcile(to: AgentState) -> Result<(), ReconcileError>;
}

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum ReconcileError {
    #[error("aborted by a more recent reconcilation request")]
    Aborted,
    #[error("failed to download the specified storage")]
    StorageAcquireError,
    #[error("unknown error")]
    Unknown,
}
