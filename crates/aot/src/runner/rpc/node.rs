#![allow(dead_code)]

use snops_common::{
    define_rpc_mux,
    rpc::{
        agent::{
            node::{NodeService, NodeServiceRequest, NodeServiceResponse},
            AgentNodeServiceRequest, AgentNodeServiceResponse,
        },
        error::AgentError,
    },
};
use tarpc::context;

use crate::cli::{make_env_filter, ReloadHandler};

define_rpc_mux!(child;
    AgentNodeServiceRequest => AgentNodeServiceResponse;
    NodeServiceRequest => NodeServiceResponse;
);

#[derive(Clone)]
pub struct NodeRpcServer {
    pub log_level_handler: ReloadHandler,
}

impl NodeService for NodeRpcServer {
    async fn set_log_level(self, _: context::Context, verbosity: u8) -> Result<(), AgentError> {
        tracing::debug!("NodeService Setting log verbosity {verbosity:?}");

        self.log_level_handler
            .modify(|filter| *filter = make_env_filter(verbosity))
            .map_err(|_| AgentError::FailedToChangeLogLevel)?;

        Ok(())
    }
}