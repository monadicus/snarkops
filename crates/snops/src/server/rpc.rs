use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use chrono::Utc;
use snops_common::{
    api::EnvInfo,
    rpc::{
        agent::{AgentServiceRequest, AgentServiceResponse},
        control::{ControlService, ControlServiceRequest, ControlServiceResponse},
        error::ResolveError,
        MuxMessage,
    },
    state::{
        AgentId, AgentState, EnvId, LatestBlockInfo, NodeStatus, TransferStatus,
        TransferStatusUpdate,
    },
};
use tarpc::{context, ClientMessage, Response};

use super::AppState;
use crate::{
    error::StateError,
    state::{AddrMap, AgentAddrs},
};

/// A multiplexed message, incoming on the websocket.
pub type MuxedMessageIncoming =
    MuxMessage<ClientMessage<ControlServiceRequest>, Response<AgentServiceResponse>>;

/// A multiplexed message, outgoing on the websocket.
pub type MuxedMessageOutgoing =
    MuxMessage<Response<ControlServiceResponse>, ClientMessage<AgentServiceRequest>>;

#[derive(Clone)]
pub struct ControlRpcServer {
    pub state: AppState,
    pub agent: AgentId,
}

impl ControlService for ControlRpcServer {
    async fn resolve_addrs(
        self,
        _: context::Context,
        mut peers: HashSet<AgentId>,
    ) -> Result<HashMap<AgentId, IpAddr>, ResolveError> {
        peers.insert(self.agent);

        let addr_map = self
            .state
            .get_addr_map(Some(&peers))
            .await
            .map_err(|_| ResolveError::AgentHasNoAddresses)?;
        resolve_addrs(&addr_map, self.agent, &peers).map_err(|_| ResolveError::SourceAgentNotFound)
    }

    async fn get_env_info(self, _: context::Context, env_id: EnvId) -> Option<EnvInfo> {
        Some(self.state.get_env(env_id)?.info())
    }

    async fn post_transfer_status(
        self,
        _: context::Context,
        id: u32,
        update: TransferStatusUpdate,
    ) {
        let Some(mut agent) = self.state.pool.get_mut(&self.agent) else {
            return;
        };

        // patch the agent's transfer status
        match (update, agent.status.transfers.get_mut(&id)) {
            (TransferStatusUpdate::Start { desc, time, total }, None) => {
                agent.status.transfers.insert(
                    id,
                    TransferStatus {
                        desc,
                        started_at: time,
                        updated_at: Utc::now(),
                        downloaded_bytes: 0,
                        total_bytes: total,
                        interruption: None,
                    },
                );
            }
            (TransferStatusUpdate::Progress { downloaded }, Some(transfer)) => {
                transfer.downloaded_bytes = downloaded;
                transfer.updated_at = Utc::now();
            }
            (TransferStatusUpdate::End { interruption }, Some(transfer)) => {
                if interruption.is_none() {
                    transfer.downloaded_bytes = transfer.total_bytes;
                }
                transfer.interruption = interruption;
                transfer.updated_at = Utc::now();
            }
            (TransferStatusUpdate::Cleanup, mut status @ Some(_)) => {
                status.take();
            }

            _ => {}
        }
    }

    async fn post_transfer_statuses(
        self,
        _: context::Context,
        statuses: Vec<(u32, TransferStatus)>,
    ) {
        let Some(mut agent) = self.state.pool.get_mut(&self.agent) else {
            return;
        };

        agent.status.transfers = statuses.into_iter().collect();
    }

    async fn post_block_status(
        self,
        _: context::Context,
        height: u32,
        timestamp: i64,
        state_root: String,
        block_hash: String,
    ) {
        let Some(mut agent) = self.state.pool.get_mut(&self.agent) else {
            return;
        };

        let AgentState::Node(env_id, _) = agent.state() else {
            return;
        };

        let info = LatestBlockInfo {
            height,
            state_root,
            block_hash,
            block_timestamp: timestamp,
            update_time: Utc::now(),
        };

        self.state.update_env_block_info(*env_id, &info);
        agent.status.block_info = Some(info);
    }

    async fn post_node_status(self, _: context::Context, status: NodeStatus) {
        let Some(mut agent) = self.state.pool.get_mut(&self.agent) else {
            return;
        };

        agent.status.node_status = status;
    }
}

/// Given a map of addresses, resolve the addresses of a set of peers relative
/// to a source agent.
fn resolve_addrs(
    addr_map: &AddrMap,
    src: AgentId,
    peers: &HashSet<AgentId>,
) -> Result<HashMap<AgentId, IpAddr>, StateError> {
    let src_addrs = addr_map
        .get(&src)
        .ok_or_else(|| StateError::SourceAgentNotFound(src))?;

    let all_internal = addr_map
        .values()
        .all(|AgentAddrs { external, .. }| external.is_none());

    Ok(peers
        .iter()
        .filter_map(|id| {
            // ignore the source agent
            if *id == src {
                return None;
            }

            // if the agent has no addresses, skip it
            let addrs = addr_map.get(id)?;

            // if there are no external addresses in the entire addr map,
            // use the first internal address
            if all_internal {
                return addrs.internal.first().copied().map(|addr| (*id, addr));
            }

            match (src_addrs.external, addrs.external, addrs.internal.first()) {
                // if peers have the same external address, use the first internal address
                (Some(src_ext), Some(peer_ext), Some(peer_int)) if src_ext == peer_ext => {
                    Some((*id, *peer_int))
                }
                // otherwise use the external address
                (_, Some(peer_ext), _) => Some((*id, peer_ext)),
                _ => None,
            }
        })
        .collect())
}
