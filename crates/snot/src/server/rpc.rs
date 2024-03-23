use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use snot_common::{
    rpc::{
        agent::{AgentServiceRequest, AgentServiceResponse},
        control::{ControlService, ControlServiceRequest, ControlServiceResponse, ResolveError},
        MuxMessage,
    },
    state::AgentId,
};
use tarpc::{context, ClientMessage, Response};

use super::AppState;
use crate::state::resolve_addrs;

/// A multiplexed message, incoming on the websocket.
pub type MuxedMessageIncoming =
    MuxMessage<ClientMessage<ControlServiceRequest>, Response<AgentServiceResponse>>;

/// A multiplexed message, outgoing on the websocket.
pub type MuxedMessageOutgoing =
    MuxMessage<Response<ControlServiceResponse>, ClientMessage<AgentServiceRequest>>;

#[derive(Clone)]
pub struct ControlRpcServer {
    pub state: AppState,
    pub agent: usize,
}

impl ControlService for ControlRpcServer {
    async fn placeholder(self, _: context::Context) -> String {
        "Hello, world".into()
    }

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
}
