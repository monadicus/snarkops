use std::time::Duration;

use snops_common::{
    rpc::{agent::AgentServiceClient, error::ReconcileError},
    state::{AgentState, EnvId, NetworkId},
};
use tarpc::{client::RpcError, context};

use crate::error::StateError;

#[derive(Clone)]
pub struct AgentClient(pub(super) AgentServiceClient);

impl AgentClient {
    pub async fn reconcile(
        &self,
        to: AgentState,
    ) -> Result<Result<AgentState, ReconcileError>, RpcError> {
        let mut ctx = context::current();
        ctx.deadline += Duration::from_secs(300);
        self.0
            .reconcile(ctx, to.clone())
            .await
            .map(|res| res.map(|_| to))
    }

    pub async fn get_state_root(&self) -> Result<String, StateError> {
        Ok(self.0.get_state_root(context::current()).await??)
    }

    pub async fn execute_authorization(
        &self,
        env_id: EnvId,
        network: NetworkId,
        query: String,
        auth: String,
        fee_auth: Option<String>,
    ) -> Result<(), StateError> {
        Ok(self
            .0
            .execute_authorization(context::current(), env_id, network, query, auth, fee_auth)
            .await??)
    }

    pub async fn broadcast_tx(&self, tx: String) -> Result<(), StateError> {
        Ok(self.0.broadcast_tx(context::current(), tx).await??)
    }
}
