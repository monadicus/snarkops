//! Agent-to-node RPC.

use snops_common::{
    define_rpc_mux,
    rpc::agent::{
        node::{NodeServiceRequest, NodeServiceResponse},
        AgentNodeService, AgentNodeServiceRequest, AgentNodeServiceResponse,
    },
    state::snarkos_status::{SnarkOSBlockInfo, SnarkOSStatus},
};
use tarpc::context;

use crate::state::AppState;

define_rpc_mux!(parent;
    AgentNodeServiceRequest => AgentNodeServiceResponse;
    NodeServiceRequest => NodeServiceResponse;
);

#[derive(Clone)]
pub struct AgentNodeRpcServer {
    pub state: AppState,
}

impl AgentNodeService for AgentNodeRpcServer {
    async fn post_block_info(
        self,
        _: context::Context,
        SnarkOSBlockInfo {
            height,
            state_root,
            block_hash,
            previous_hash: prev_block_hash,
            block_timestamp,
        }: SnarkOSBlockInfo,
    ) -> Result<(), ()> {
        let Some(client) = self.state.client.read().await.clone() else {
            return Ok(()); // ignore if client is not available
        };

        client
            .post_block_status(
                context::current(),
                height,
                block_timestamp,
                state_root,
                block_hash,
                prev_block_hash,
            )
            .await
            .inspect_err(|err| tracing::error!("failed to post block status: {err}"))
            .map_err(|_| ())
    }

    async fn post_status(self, _: context::Context, status: SnarkOSStatus) -> Result<(), ()> {
        let Some(client) = self.state.client.read().await.clone() else {
            return Ok(()); // ignore if client is not available
        };

        // Update the last node status
        self.state.set_node_status(Some(status.clone())).await;

        client
            .post_node_status(context::current(), status.into())
            .await
            .inspect_err(|err| tracing::error!("failed to post node status: {err}"))
            .map_err(|_| ())
    }
}
