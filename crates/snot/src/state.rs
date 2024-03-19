use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

use snot_common::{
    rpc::{AgentServiceClient, RpcErrorOr},
    state::{AgentState, NodeState},
};
use tarpc::context;
use tokio::sync::RwLock;

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
    rpc: AgentServiceClient,
    state: Option<NodeState>,
}

pub struct AgentClient(AgentServiceClient);

impl Agent {
    pub fn new(rpc: AgentServiceClient) -> Self {
        static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

        let id = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            rpc,
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

    /// Get an owned copy of the RPC client for making reconcilation calls.
    pub fn client(&self) -> AgentClient {
        AgentClient(self.rpc.to_owned())
    }
}

impl AgentClient {
    pub async fn reconcile(&self, to: AgentState) -> Result<(), RpcErrorOr> {
        self.0
            .reconcile(context::current(), to)
            .await?
            .map_err(|_| RpcErrorOr::Other(()))
    }
}
