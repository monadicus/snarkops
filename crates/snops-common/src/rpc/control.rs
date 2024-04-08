use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use super::error::ResolveError;
use crate::state::AgentId;

#[tarpc::service]
pub trait ControlService {
    /// Resolve the addresses of the given agents.
    async fn resolve_addrs(
        peers: HashSet<AgentId>,
    ) -> Result<HashMap<AgentId, IpAddr>, ResolveError>;
}
