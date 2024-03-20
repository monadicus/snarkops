use std::{ops::Deref, sync::Arc};

use snot_common::{
    rpc::{
        agent::{AgentService, AgentServiceRequest, AgentServiceResponse, ReconcileError},
        control::{ControlServiceRequest, ControlServiceResponse},
        MuxMessage,
    },
    state::{AgentState, NodeType},
};
use tarpc::{context, ClientMessage, Response};
use tokio::process::Command;
use tracing::{debug, warn};

use crate::state::AppState;

pub const JWT_FILE: &str = "jwt";
pub const SNARKOS_FILE: &str = "snarkos";

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
        tokio::fs::write(self.state.cli.path.join(JWT_FILE), token)
            .await
            .expect("failed to write jwt file");
    }

    async fn reconcile(
        self,
        _: context::Context,
        target: AgentState,
    ) -> Result<(), ReconcileError> {
        if matches!(target, AgentState::Cannon(_, _)) {
            unimplemented!("tx cannons are unimplemented");
        }

        // acquire the handle lock
        let mut handle_container = self.state.reconcilation_handle.lock().await;

        // abort if we are already reconciling
        if let Some(handle) = handle_container.take() {
            handle.abort();
        }

        // perform the reconcilation
        let state = Arc::clone(&self.state);
        let handle = tokio::spawn(async move {
            // previous state cleanup
            match state.agent_state.read().await.deref() {
                // kill existing child if running
                AgentState::Node(_, node) if node.online => {
                    if let Some(mut child) = state.child.write().await.take() {
                        child.kill().await.expect("failed to kill child process");
                    }
                }

                _ => (),
            }

            // reconcile towards new state
            match target {
                // do nothing on inventory state
                AgentState::Inventory => (),

                // start snarkOS node when node
                AgentState::Node(_storage_id, node) => {
                    // TODO: refer to proper storage_id
                    // TODO: we may want a separate, abortable task to handle the execution of this
                    // child, so that we can properly track its stdout. we can use a similar
                    // technique for this by storing a JoinHandle<()>/AbortHandle in the GlobalState
                    // and aborting it when we want to kill the child.
                    //
                    // in order to kill the child when we drop the task that executes it, we can use
                    // `Command::kill_on_drop`

                    let mut child_lock = state.child.write().await;
                    let child = Command::new(state.cli.path.join(SNARKOS_FILE))
                        // TODO: more args
                        .arg(match node.ty {
                            NodeType::Client => "--client",
                            NodeType::Prover => "--prover",
                            NodeType::Validator => "--validator",
                        })
                        .spawn()
                        .expect("failed to start child");

                    *child_lock = Some(child);
                }

                AgentState::Cannon(_, _) => unimplemented!(),
            }
        });

        // update the mutex with our new handle and drop the lock
        *handle_container = Some(handle.abort_handle());
        drop(handle_container);

        // await reconcilation completion
        let res = match handle.await {
            Err(e) if e.is_cancelled() => {
                warn!("reconcilation was aborted by a newer reconcilation request");

                // early return (don't clean up the handle lock)
                return Err(ReconcileError::Aborted);
            }

            Ok(()) => Ok(()),
            Err(e) => {
                warn!("reconcilation task panicked: {e}");
                Err(ReconcileError::Unknown)
            }
        };

        // clean up the abort handle
        // we can't be here if we were cancelled (see early return above)
        self.state.reconcilation_handle.lock().await.take();

        res
    }
}
