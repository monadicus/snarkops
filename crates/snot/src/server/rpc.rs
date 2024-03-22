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

use crate::state::resolve_addrs;

use super::AppState;

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
        peers: HashSet<AgentId>,
    ) -> Result<HashMap<AgentId, IpAddr>, ResolveError> {
        let addr_map = self
            .state
            .get_addr_map(Some(&peers))
            .await
            .map_err(|_| ResolveError::AgentHasNoAddresses)?;
        resolve_addrs(&addr_map, self.agent, &peers)
            .await
            .map_err(|_| ResolveError::AgentHasNoAddresses)
    }
}
