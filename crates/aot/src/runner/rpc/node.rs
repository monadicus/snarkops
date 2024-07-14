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
use tracing::level_filters::LevelFilter;

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
    async fn set_log_level(
        self,
        _: context::Context,
        level: Option<String>,
        verbosity: Option<u8>,
    ) -> Result<(), AgentError> {
        let level: Option<LevelFilter> = level
            .as_ref()
            .map(|l| l.parse())
            .transpose()
            .map_err(|_| AgentError::InvalidLogLevel(level.unwrap_or_default()))?;

        self.log_level_handler
            .modify(|filter| *filter = make_env_filter(level, verbosity))
            .expect("failed to set log level");
        Ok(())
    }
}
