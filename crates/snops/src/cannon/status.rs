use snops_common::state::AgentId;
use tokio::sync::mpsc::Sender;

pub struct TransactionStatusSender(Option<Sender<TransactionStatus>>);

impl TransactionStatusSender {
    pub fn new(sender: Sender<TransactionStatus>) -> Self {
        Self(Some(sender))
    }

    pub fn empty() -> Self {
        Self(None)
    }

    pub fn send(&self, status: TransactionStatus) {
        if let Some(sender) = &self.0 {
            let _ = sender.try_send(status);
        }
    }
}

pub enum TransactionStatus {
    /// Authorization has been aborted
    ExecuteAborted,
    /// Authorization has been queued for execution.
    ExecuteQueued,
    /// No agents are available for the execution
    ExecuteAwaitingCompute,
    /// An agent was found and the authorization is being executed
    Executing(AgentId),
    /// Execute RPC failed
    ExecuteFailed(String),
    /// Agent has completed the execution
    ExecuteComplete(String),
    // TODO: Implement the following statuses
    // /// API has received the transaction broadcast
    // BroadcastReceived,
    // /// Control plane has forwarded the transaction to a peer
    // BroadcastForwarded,
    // /// An error occurred while broadcasting the transaction
    // BroadcastFailed,
    // /// Transaction was found in the network, return the block hash
    // TransactionConfirmed(String),
}
