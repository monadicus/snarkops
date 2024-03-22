use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::state::AgentId;

#[tarpc::service]
pub trait ControlService {
    async fn placeholder() -> String;

    /// Resolve the addresses of the given agents.
    async fn resolve_addrs(
        peers: HashSet<AgentId>,
    ) -> Result<HashMap<AgentId, IpAddr>, ResolveError>;
}

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum ResolveError {
    #[error("source agent not found")]
    SourceAgentNotFound,
    #[error("agent has no addresses")]
    AgentHasNoAddresses,
}
