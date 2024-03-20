use snot_common::{
    rpc::{
        AgentService, AgentServiceRequest, AgentServiceResponse, ControlServiceRequest,
        ControlServiceResponse, MuxMessage,
    },
    state::AgentState,
};
use tarpc::{context, ClientMessage, Response};
use tokio::{fs::File, io::AsyncWriteExt};
use tracing::{debug, info};

/// A multiplexed message, incoming on the websocket.
pub type MuxedMessageIncoming =
    MuxMessage<Response<ControlServiceResponse>, ClientMessage<AgentServiceRequest>>;

/// A multiplexed message, outgoing on the websocket.
pub type MuxedMessageOutgoing =
    MuxMessage<ClientMessage<ControlServiceRequest>, Response<AgentServiceResponse>>;

// TODO: include agent state (process, JWT, etc.)
#[derive(Clone)]
pub struct AgentRpcServer;

impl AgentService for AgentRpcServer {
    async fn keep_jwt(self, _: context::Context, token: String) {
        debug!("control plane delegated new JWT");

        // TODO: write the JWT to a file somewhere else
        // TODO: cache the JWT in-memory
        tokio::fs::write("./jwt.txt", token)
            .await
            .expect("failed to write jwt file");
    }

    async fn reconcile(self, _: context::Context, state: AgentState) -> Result<(), ()> {
        info!("I've been asked to reconcile to {state:#?}");

        Ok(())
    }
}
