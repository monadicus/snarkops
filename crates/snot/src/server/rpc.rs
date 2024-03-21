use snot_common::rpc::{
    agent::{AgentServiceRequest, AgentServiceResponse},
    control::{ControlService, ControlServiceRequest, ControlServiceResponse},
    MuxMessage,
};
use tarpc::{context, ClientMessage, Response};

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
}
