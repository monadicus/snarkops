#![allow(dead_code)]

use snops_common::{
    define_rpc_mux,
    rpc::agent::{
        node::{NodeService, NodeServiceRequest, NodeServiceResponse},
        AgentNodeServiceRequest, AgentNodeServiceResponse,
    },
};
use tarpc::context;

define_rpc_mux!(child;
    AgentNodeServiceRequest => AgentNodeServiceResponse;
    NodeServiceRequest => NodeServiceResponse;
);

#[derive(Clone)]
pub struct NodeRpcServer {
    pub state: (),
}

impl NodeService for NodeRpcServer {
    async fn foo(self, _: context::Context) {
        // gotta have some dummy method here for tarpc to compile
    }
}
