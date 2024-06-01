use std::{fmt::Display, time::Duration};

use serde::de::DeserializeOwned;
use snops_common::{
    rpc::{
        agent::AgentServiceClient,
        error::{ReconcileError, SnarkosRequestError},
    },
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

    pub async fn snarkos_get<T: DeserializeOwned>(
        &self,
        route: impl Display,
    ) -> Result<T, SnarkosRequestError> {
        match self
            .0
            .snarkos_get(context::current(), route.to_string())
            .await
        {
            Ok(res) => serde_json::from_str(&res?)
                .map_err(|e| SnarkosRequestError::JsonDeserializeError(e.to_string())),
            Err(e) => Err(SnarkosRequestError::RpcError(e.to_string())),
        }
    }

    pub async fn execute_authorization(
        &self,
        env_id: EnvId,
        network: NetworkId,
        query: String,
        auth: String,
        fee_auth: Option<String>,
    ) -> Result<String, StateError> {
        Ok(self
            .0
            .execute_authorization(context::current(), env_id, network, query, auth, fee_auth)
            .await??)
    }

    pub async fn broadcast_tx(&self, tx: String) -> Result<(), StateError> {
        Ok(self.0.broadcast_tx(context::current(), tx).await??)
    }
}
