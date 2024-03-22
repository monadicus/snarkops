use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};

use bimap::BiMap;
use jwt::SignWithKey;
use snot_common::{
    rpc::agent::{AgentServiceClient, ReconcileError},
    state::AgentState,
};
use tarpc::{client::RpcError, context};
use tokio::sync::RwLock;

use crate::{
    cli::Cli,
    server::jwt::{Claims, JWT_NONCE, JWT_SECRET},
    testing::Test,
};

pub type AgentId = usize;

pub type AppState = Arc<GlobalState>;

/// The global state for the control plane.
#[derive(Debug)]
pub struct GlobalState {
    pub cli: Cli,
    pub pool: RwLock<HashMap<AgentId, Agent>>,
    /// A map from ephemeral integer storage ID to actual storage ID.
    pub storage: RwLock<BiMap<usize, String>>,
    pub test: RwLock<Option<Test>>,
}

/// An active agent, known by the control plane.
#[derive(Debug)]
pub struct Agent {
    id: AgentId,
    claims: Claims,
    connection: AgentConnection,
    state: AgentState,
}

pub struct AgentClient(AgentServiceClient);

impl Agent {
    pub fn new(rpc: AgentServiceClient) -> Self {
        static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = ID_COUNTER.fetch_add(1, Ordering::Relaxed);

        Self {
            id,
            claims: Claims {
                id,
                nonce: *JWT_NONCE,
            },
            connection: AgentConnection::Online(rpc),
            state: Default::default(),
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.connection, AgentConnection::Online(_))
    }

    /// The ID of this agent.
    pub fn id(&self) -> usize {
        self.id
    }

    /// The current state of this agent.
    pub fn state(&self) -> &AgentState {
        &self.state
    }

    pub fn claims(&self) -> &Claims {
        &self.claims
    }

    pub fn sign_jwt(&self) -> String {
        self.claims.to_owned().sign_with_key(&*JWT_SECRET).unwrap()
    }

    pub fn rpc(&self) -> Option<&AgentServiceClient> {
        match self.connection {
            AgentConnection::Online(ref rpc) => Some(rpc),
            _ => None,
        }
    }

    /// Get an owned copy of the RPC client for making reconcilation calls.
    /// `None` if the client is not currently connected.
    pub fn client_owned(&self) -> Option<AgentClient> {
        match self.connection {
            AgentConnection::Online(ref rpc) => Some(AgentClient(rpc.to_owned())),
            _ => None,
        }
    }

    /// Forcibly remove the RPC connection to this client. Called when an agent
    /// disconnects.
    pub fn mark_disconnected(&mut self) {
        self.connection = AgentConnection::Offline {
            since: Instant::now(),
        };
    }

    pub fn mark_connected(&mut self, client: AgentServiceClient) {
        self.connection = AgentConnection::Online(client);
    }

    /// Forcibly sets an agent's state. This does **not** reconcile the agent,
    /// and should only be called after an agent is reconciled.
    pub fn set_state(&mut self, state: AgentState) {
        self.state = state;
    }
}

impl AgentClient {
    pub async fn reconcile(&self, to: AgentState) -> Result<Result<(), ReconcileError>, RpcError> {
        self.0.reconcile(context::current(), to).await
    }
}

#[derive(Debug, Clone)]
pub enum AgentConnection {
    Online(AgentServiceClient),
    Offline { since: Instant },
}
