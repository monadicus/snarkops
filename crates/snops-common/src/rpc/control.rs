use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use super::error::ResolveError;
use crate::{
    api::EnvInfo,
    state::{AgentId, EnvId},
};

#[tarpc::service]
pub trait ControlService {
    /// Resolve the addresses of the given agents.
    async fn resolve_addrs(
        peers: HashSet<AgentId>,
    ) -> Result<HashMap<AgentId, IpAddr>, ResolveError>;

    /// Get the environment info for the given environment.
    async fn get_env_info(env_id: EnvId) -> Option<EnvInfo>;
}
