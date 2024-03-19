use snot_common::{
    rpc::{
        AgentService, AgentServiceRequest, AgentServiceResponse, ControlServiceRequest,
        ControlServiceResponse, MuxMessage,
    },
    state::AgentState,
};
use tarpc::{context, ClientMessage, Response};
use tracing::info;

/// A multiplexed message, incoming on the websocket.
pub type MuxedMessageIncoming =
    MuxMessage<Response<ControlServiceResponse>, ClientMessage<AgentServiceRequest>>;

/// A multiplexed message, outgoing on the websocket.
pub type MuxedMessageOutgoing =
    MuxMessage<ClientMessage<ControlServiceRequest>, Response<AgentServiceResponse>>;

#[derive(Clone)]
pub struct AgentRpcServer;

impl AgentService for AgentRpcServer {
    async fn reconcile(self, _: context::Context, state: AgentState) -> Result<(), ()> {
        info!("I've been asked to reconcile to {state:#?}");

        Ok(())
    }
}
