use std::time::Duration;

use snot_common::{rpc::AgentService, state::AgentState};
use tarpc::context;
use tracing::info;

#[derive(Clone)]
pub struct AgentRpcServer;

impl AgentService for AgentRpcServer {
    async fn reconcile(self, _: context::Context, state: AgentState) -> Result<(), ()> {
        info!("I've been asked to reconcile to {state:#?}");

        Ok(())
    }

    async fn test_reverse_string(self, _: context::Context, msg: String) -> String {
        tokio::time::sleep(Duration::from_secs(2)).await;
        msg.chars().rev().collect()
    }
}
