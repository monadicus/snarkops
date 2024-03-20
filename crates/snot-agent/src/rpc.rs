use snot_common::{
    rpc::{
        AgentService, AgentServiceRequest, AgentServiceResponse, ControlServiceRequest,
        ControlServiceResponse, MuxMessage,
    },
    state::AgentState,
};
use tarpc::{context, ClientMessage, Response};
use tracing::{debug, info};

use crate::state::AppState;

/// A multiplexed message, incoming on the websocket.
pub type MuxedMessageIncoming =
    MuxMessage<Response<ControlServiceResponse>, ClientMessage<AgentServiceRequest>>;

/// A multiplexed message, outgoing on the websocket.
pub type MuxedMessageOutgoing =
    MuxMessage<ClientMessage<ControlServiceRequest>, Response<AgentServiceResponse>>;

// TODO: include agent state (process, JWT, etc.)
#[derive(Clone)]
pub struct AgentRpcServer {
    pub state: AppState,
}

impl AgentService for AgentRpcServer {
    async fn keep_jwt(self, _: context::Context, token: String) {
        debug!("control plane delegated new JWT");

        // cache the JWT in the state JWT mutex
        self.state
            .jwt
            .lock()
            .expect("failed to acquire JWT lock")
            .replace(token.to_owned());

        // TODO: write the JWT to a file somewhere else
        tokio::fs::write("./jwt.txt", token)
            .await
            .expect("failed to write jwt file");
    }

    async fn reconcile(self, _: context::Context, state: AgentState) -> Result<(), ()> {
        info!("I've been asked to reconcile to {state:#?}");

        Ok(())
    }
}
