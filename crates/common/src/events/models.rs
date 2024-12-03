use std::{fmt::Display, str::FromStr, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::EventFilter;
use crate::{
    aot_cmds::Authorization,
    rpc::error::ReconcileError,
    state::{
        AgentId, EnvId, InternedId, LatestBlockInfo, NodeKey, NodeStatus, ReconcileStatus,
        TransactionSendState,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum EventWsRequest {
    Subscribe { id: u32, filter: EventFilter },
    Unsubscribe { id: u32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_key: Option<NodeKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<EnvId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction: Option<Arc<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cannon: Option<InternedId>,
    pub content: EventKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    Agent(AgentEvent),
    Transaction(TransactionEvent),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    /// An agent connects to the control plane
    Connected,
    /// An agent completes a handshake with the control plane
    HandshakeComplete,
    /// An agent disconnects from the control plane
    Disconnected,
    /// An agent finishes a reconcile
    ReconcileComplete,
    /// An agent updates its reconcile status
    Reconcile(ReconcileStatus<()>),
    /// An error occurs during reconcile
    ReconcileError(ReconcileError),
    /// An agent emits a node status
    NodeStatus(NodeStatus),
    /// An agent emits a block update
    BlockInfo(LatestBlockInfo),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransactionEvent {
    /// The authorization was inserted into the cannon
    AuthorizationReceived { authorization: Arc<Authorization> },
    /// The transaction execution was aborted
    ExecuteAborted(TransactionAbortReason),
    /// The transaction is awaiting compute resources
    ExecuteAwaitingCompute,
    /// An execution failed to complete after multiple attempts
    ExecuteExceeded { attempts: u32 },
    /// The transaction execution failed
    ExecuteFailed(String),
    /// The transaction is currently executing
    Executing,
    /// The transaction execution is complete
    ExecuteComplete { transaction: Arc<serde_json::Value> },
    /// The transaction has been broadcasted
    Broadcasted {
        height: Option<u32>,
        timestamp: DateTime<Utc>,
    },
    /// The transaction broadcast has exceeded the maximum number of attempts
    BroadcastExceeded { attempts: u32 },
    /// The transaction has been confirmed by the network
    Confirmed { hash: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum TransactionAbortReason {
    MissingTracker,
    UnexpectedStatus(TransactionSendState),
    MissingAuthorization,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventKindFilter {
    AgentConnected,
    AgentHandshakeComplete,
    AgentDisconnected,
    AgentReconcileComplete,
    AgentReconcile,
    AgentReconcileError,
    AgentNodeStatus,
    AgentBlockInfo,
    TransactionAuthorizationReceived,
    TransactionExecuteAborted,
    TransactionExecuteAwaitingCompute,
    TransactionExecuteExceeded,
    TransactionExecuteFailed,
    TransactionExecuting,
    TransactionExecuteComplete,
    TransactionBroadcasted,
    TransactionBroadcastExceeded,
    TransactionConfirmed,
}

impl EventKind {
    pub fn filter(&self) -> EventKindFilter {
        use AgentEvent::*;
        use EventKind::*;
        use EventKindFilter::*;
        use TransactionEvent::*;

        match self {
            Agent(Connected) => AgentConnected,
            Agent(HandshakeComplete) => AgentHandshakeComplete,
            Agent(Disconnected) => AgentDisconnected,
            Agent(ReconcileComplete) => AgentReconcileComplete,
            Agent(Reconcile(_)) => AgentReconcile,
            Agent(ReconcileError(_)) => AgentReconcileError,
            Agent(NodeStatus(_)) => AgentNodeStatus,
            Agent(BlockInfo(_)) => AgentBlockInfo,
            Transaction(AuthorizationReceived { .. }) => TransactionAuthorizationReceived,
            Transaction(ExecuteAborted(_)) => TransactionExecuteAborted,
            Transaction(ExecuteAwaitingCompute) => TransactionExecuteAwaitingCompute,
            Transaction(ExecuteExceeded { .. }) => TransactionExecuteExceeded,
            Transaction(ExecuteFailed(_)) => TransactionExecuteFailed,
            Transaction(Executing) => TransactionExecuting,
            Transaction(ExecuteComplete { .. }) => TransactionExecuteComplete,
            Transaction(Broadcasted { .. }) => TransactionBroadcasted,
            Transaction(BroadcastExceeded { .. }) => TransactionBroadcastExceeded,
            Transaction(Confirmed { .. }) => TransactionConfirmed,
        }
    }
}

impl FromStr for EventKindFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            // kebab-case
            "agent-connected" => Ok(Self::AgentConnected),
            "agent-handshake-complete" => Ok(Self::AgentHandshakeComplete),
            "agent-disconnected" => Ok(Self::AgentDisconnected),
            "agent-reconcile-complete" => Ok(Self::AgentReconcileComplete),
            "agent-reconcile" => Ok(Self::AgentReconcile),
            "agent-reconcile-error" => Ok(Self::AgentReconcileError),
            "agent-node-status" => Ok(Self::AgentNodeStatus),
            "agent-block-info" => Ok(Self::AgentBlockInfo),
            "transaction-authorization-received" => Ok(Self::TransactionAuthorizationReceived),
            "transaction-execute-aborted" => Ok(Self::TransactionExecuteAborted),
            "transaction-execute-awaiting-compute" => Ok(Self::TransactionExecuteAwaitingCompute),
            "transaction-execute-exceeded" => Ok(Self::TransactionExecuteExceeded),
            "transaction-execute-failed" => Ok(Self::TransactionExecuteFailed),
            "transaction-executing" => Ok(Self::TransactionExecuting),
            "transaction-execute-complete" => Ok(Self::TransactionExecuteComplete),
            "transaction-broadcasted" => Ok(Self::TransactionBroadcasted),
            "transaction-broadcast-exceeded" => Ok(Self::TransactionBroadcastExceeded),
            "transaction-confirmed" => Ok(Self::TransactionConfirmed),
            _ => Err(format!("invalid event kind: {s}")),
        }
    }
}

impl Display for EventKindFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use EventKindFilter::*;

        let s = match self {
            AgentConnected => "agent-connected",
            AgentHandshakeComplete => "agent-handshake-complete",
            AgentDisconnected => "agent-disconnected",
            AgentReconcileComplete => "agent-reconcile-complete",
            AgentReconcile => "agent-reconcile",
            AgentReconcileError => "agent-reconcile-error",
            AgentNodeStatus => "agent-node-status",
            AgentBlockInfo => "agent-block-info",
            TransactionAuthorizationReceived => "transaction-authorization-received",
            TransactionExecuteAborted => "transaction-execute-aborted",
            TransactionExecuteAwaitingCompute => "transaction-execute-awaiting-compute",
            TransactionExecuteExceeded => "transaction-execute-exceeded",
            TransactionExecuteFailed => "transaction-execute-failed",
            TransactionExecuting => "transaction-executing",
            TransactionExecuteComplete => "transaction-execute-complete",
            TransactionBroadcasted => "transaction-broadcasted",
            TransactionBroadcastExceeded => "transaction-broadcast-exceeded",
            TransactionConfirmed => "transaction-confirmed",
        };

        write!(f, "{}", s)
    }
}

impl Event {
    pub fn new(content: EventKind) -> Self {
        Self {
            created_at: Utc::now(),
            agent: None,
            node_key: None,
            env: None,
            transaction: None,
            cannon: None,
            content,
        }
    }

    pub fn replace_content(&self, content: impl Into<Event>) -> Self {
        Self {
            created_at: Utc::now(),
            agent: self.agent,
            node_key: self.node_key.clone(),
            env: self.env,
            transaction: self.transaction.clone(),
            cannon: self.cannon,
            content: content.into().content,
        }
    }
}
