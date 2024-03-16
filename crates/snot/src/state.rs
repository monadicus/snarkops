use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

use snot_common::{message::ServerMessage, state::NodeState};
use tokio::sync::{mpsc, RwLock};

/// The global state for the control plane.
#[derive(Default)]
pub struct GlobalState {
    pub pool: RwLock<HashMap<usize, Agent>>,
    // TODO: when tests are running, there should be (bi-directional?) map between agent ID and
    // assigned NodeKey (like validator/1)
}

/// An active agent, known by the control plane.
#[derive(Debug)]
pub struct Agent {
    id: usize,
    tx: mpsc::Sender<ServerMessage>,
    state: Option<NodeState>, // TODO: revert state if set state fails
}

type SendResult = Result<(), mpsc::error::SendError<ServerMessage>>;

impl Agent {
    pub fn new(tx: mpsc::Sender<ServerMessage>) -> Self {
        static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

        let id = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            tx,
            state: Default::default(),
        }
    }

    /// The ID of this agent.
    pub fn id(&self) -> usize {
        self.id
    }

    /// The current desired state of this agent.
    pub fn state(&self) -> Option<&NodeState> {
        self.state.as_ref()
    }

    /// The current desired state of this agent, but owned, so that you can make
    /// edits to it before calling `set_state`.
    pub fn state_owned(&self) -> Option<NodeState> {
        self.state.clone()
    }

    /// Attempts to set the desired state of this agent. Informs the agent that
    /// the desired state has changed. This future does NOT await until
    /// reconciliation, but rather until the message has been accepted by the
    /// agent event channel.
    ///
    /// TODO: this should probably wait until reconciliation.
    pub async fn set_state(&mut self, new_state: NodeState) -> SendResult {
        // TODO: redo this to work with new NodeState
        // let old_state = std::mem::replace(&mut self.state, new_state.to_owned());

        // tell the agent to set its state to the new state
        // if let Err(e) = self.send(ServerMessage::StateReconcile(new_state)).await {
        //     // revert to old state if send fails
        //     let _ = std::mem::replace(&mut self.state, old_state);
        //     return Err(e);
        // }

        Ok(())
    }

    /// Sends a message on the agent event channel. Completes when the internal
    /// agent channel has received the message, NOT when the actual agent has
    /// received the message.
    pub async fn send(&self, message: ServerMessage) -> SendResult {
        self.tx.send(message).await
    }
}
