pub mod agent;

use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use super::error::ResolveError;
use crate::{
    api::EnvInfo,
    state::{AgentId, EnvId, NodeStatus, TransferStatus, TransferStatusUpdate},
};

pub const PING_HEADER: &[u8] = b"snops-agent";

#[tarpc::service]
pub trait ControlService {
    /// Resolve the addresses of the given agents.
    async fn resolve_addrs(
        peers: HashSet<AgentId>,
    ) -> Result<HashMap<AgentId, IpAddr>, ResolveError>;

    /// Get the environment info for the given environment.
    async fn get_env_info(env_id: EnvId) -> Option<EnvInfo>;

    /// Emit an agent transfer status update.
    async fn post_transfer_status(id: u32, status: TransferStatusUpdate);

    /// Emit current agent transfers. Will overwrite old status.
    async fn post_transfer_statuses(statuses: Vec<(u32, TransferStatus)>);

    /// Emit an agent block status update.
    async fn post_block_status(
        height: u32,
        timestamp: i64,
        state_root: String,
        block_hash: String,
        prev_block_hash: String,
    );

    /// Emit an agent node status update.
    async fn post_node_status(update: NodeStatus);
}
