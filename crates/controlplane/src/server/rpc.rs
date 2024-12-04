use std::{collections::HashMap, net::IpAddr, time::Instant};

use chrono::Utc;
use snops_common::events::AgentEvent;
use snops_common::{
    api::AgentEnvInfo,
    define_rpc_mux,
    prelude::{error::ReconcileError, ReconcileStatus},
    rpc::{
        control::{
            agent::{AgentServiceRequest, AgentServiceResponse},
            ControlService, ControlServiceRequest, ControlServiceResponse,
        },
        error::ResolveError,
    },
    state::{
        AgentId, AgentState, EnvId, LatestBlockInfo, NodeStatus, TransferStatus,
        TransferStatusUpdate,
    },
};
use tarpc::context;
use tracing::warn;

use crate::state::{AgentEventHelpers, EmitEvent};
use crate::{
    error::StateError,
    state::{AddrMap, AgentAddrs, AppState, GetGlobalState, GlobalState},
};

define_rpc_mux!(parent;
    ControlServiceRequest => ControlServiceResponse;
    AgentServiceRequest => AgentServiceResponse;
);

#[derive(Clone)]
pub struct ControlRpcServer {
    pub state: AppState,
    pub agent: AgentId,
}

impl ControlService for ControlRpcServer {
    async fn resolve_addrs(
        self,
        _: context::Context,
        mut peers: Vec<AgentId>,
    ) -> Result<HashMap<AgentId, IpAddr>, ResolveError> {
        peers.push(self.agent);

        let addr_map = self
            .state
            .get_addr_map(&peers)
            .await
            .map_err(|_| ResolveError::AgentHasNoAddresses)?;
        resolve_addrs(&addr_map, self.agent, &peers).map_err(|_| ResolveError::SourceAgentNotFound)
    }

    async fn get_env_info(self, _: context::Context, env_id: EnvId) -> Option<AgentEnvInfo> {
        Some(self.state.get_env(env_id)?.agent_info())
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
                        handle: None,
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
        prev_block_hash: String,
    ) {
        let Some(mut agent) = self.state.pool.get_mut(&self.agent) else {
            return;
        };

        let env_id = {
            let AgentState::Node(env_id, _) = agent.state() else {
                return;
            };
            *env_id
        };

        let info = LatestBlockInfo {
            height,
            state_root,
            block_hash,
            previous_hash: prev_block_hash,
            block_timestamp: timestamp,
            update_time: Utc::now(),
        };

        AgentEvent::BlockInfo(info.clone())
            .with_agent(&agent)
            .emit(&self);

        agent.status.block_info = Some(info.clone());
        let agent_id = agent.id();
        let client = agent.client_owned().clone();

        // Prevent holding the agent lock over longer operations
        drop(agent);

        // Update the block info and if it's not new, bail early.
        // Otherwise, we'll fetch the block data and update the cache.
        if !self.state.update_env_block_info(env_id, &info) {
            return;
        }

        // If the block has the transaction or the block is not recent, ignore this
        // block
        if !self.state.env_network_cache.get(&env_id).is_some_and(|c| {
            !c.has_transactions_for_block(&info.block_hash) && c.is_recent_block(height)
        }) {
            return;
        }

        let Some(client) = client else {
            // unreachable... we're in currently in the client
            return;
        };

        // make the block request, then update the cache if applicable
        match client.get_snarkos_block_lite(info.block_hash.clone()).await {
            Ok(Some(block)) => {
                let (info, transactions) = block.split();
                if let Some(mut c) = self.state.env_network_cache.get_mut(&env_id) {
                    c.add_block(info, transactions);
                }
            }
            Ok(None) => {
                warn!(
                    "env {env_id} agent {agent_id} misreported having block {}",
                    info.block_hash
                );
            }
            Err(err) => {
                warn!(
                    "env {env_id} agent {agent_id} encountered failure requesting block {}: {err}",
                    info.block_hash
                );
            }
        }
    }

    async fn post_node_status(self, _: context::Context, status: NodeStatus) {
        let Some(mut agent) = self.state.pool.get_mut(&self.agent) else {
            return;
        };

        // Prevent redundant events
        if agent.status.node_status == status {
            return;
        }

        agent.status.node_status = status.clone();
        AgentEvent::NodeStatus(status)
            .with_agent(&agent)
            .emit(&self);
    }

    async fn post_reconcile_status(
        self,
        _: context::Context,
        status: Result<ReconcileStatus<bool>, ReconcileError>,
    ) {
        let Some(mut agent) = self.state.pool.get_mut(&self.agent) else {
            return;
        };

        agent.status.reconcile = Some((Instant::now(), status.clone()));

        // Emit events for this reconcile

        let ev = AgentEvent::ReconcileComplete.with_agent(&agent);
        let is_complete = status
            .as_ref()
            .is_ok_and(|e| e.requeue_after.is_none() && e.inner.is_some());

        ev.replace_content(match status {
            Ok(res) => AgentEvent::Reconcile(res),
            Err(err) => AgentEvent::ReconcileError(err),
        })
        .emit(&self);

        if is_complete {
            ev.emit(&self);
        }
    }
}

pub fn resolve_one_addr(src_addrs: &AgentAddrs, target_addrs: &AgentAddrs) -> Option<IpAddr> {
    match (
        src_addrs.external,
        target_addrs.external,
        target_addrs.internal.first(),
    ) {
        // if peers have the same external address, use the first internal address
        (Some(src_ext), Some(peer_ext), Some(peer_int)) if src_ext == peer_ext => Some(*peer_int),
        // if both peers have only internal addresses, use the internal address
        (None, None, Some(peer_int)) => Some(*peer_int),
        // otherwise use the external address
        (_, Some(peer_ext), _) => Some(peer_ext),
        _ => None,
    }
}

/// Given a map of addresses, resolve the addresses of a set of peers relative
/// to a source agent.
fn resolve_addrs(
    addr_map: &AddrMap,
    src: AgentId,
    peers: &[AgentId],
) -> Result<HashMap<AgentId, IpAddr>, StateError> {
    let src_addrs = addr_map
        .get(&src)
        .ok_or_else(|| StateError::SourceAgentNotFound(src))?;

    Ok(peers
        .iter()
        .filter_map(|id| {
            // ignore the source agent
            if *id == src {
                return None;
            }

            Some((*id, resolve_one_addr(src_addrs, addr_map.get(id)?)?))
        })
        .collect())
}

impl<'a> GetGlobalState<'a> for &'a ControlRpcServer {
    fn global_state(self) -> &'a GlobalState {
        &self.state
    }
}
