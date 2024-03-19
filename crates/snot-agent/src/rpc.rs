use std::time::Duration;

use snot_common::{rpc::AgentService, state::AgentState};
use tarpc::context;

#[derive(Clone)]
pub struct AgentRpcServer;

impl AgentService for AgentRpcServer {
    async fn reconcile(self, _: context::Context, _: AgentState) -> Result<(), ()> {
        Ok(())
    }

    async fn test_reverse_string(self, _: context::Context, msg: String) -> String {
        tokio::time::sleep(Duration::from_secs(2)).await;
        msg.chars().rev().collect()
    }
}
