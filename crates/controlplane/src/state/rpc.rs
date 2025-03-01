use std::{fmt::Display, time::Duration};

use serde::de::DeserializeOwned;
use snops_common::{
    rpc::{control::agent::AgentServiceClient, error::SnarkosRequestError},
    state::{
        AgentId, AgentState, EnvId, NetworkId, ReconcileOptions, snarkos_status::SnarkOSLiteBlock,
    },
};
use tarpc::{client::RpcError, context};

use crate::error::StateError;

#[derive(Clone)]
pub struct AgentClient(pub(crate) AgentServiceClient);

impl AgentClient {
    pub async fn set_agent_state(
        &self,
        to: AgentState,
        opts: ReconcileOptions,
    ) -> Result<(), RpcError> {
        self.0.set_agent_state(context::current(), to, opts).await
    }

    pub async fn clear_peer_addr(&self, peer: AgentId) -> Result<(), RpcError> {
        self.0.clear_peer_addr(context::current(), peer).await
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
    ) -> Result<String, StateError> {
        let mut ctx = context::current();
        ctx.deadline += Duration::from_secs(30);
        Ok(self
            .0
            .execute_authorization(ctx, env_id, network, query, auth)
            .await??)
    }

    pub async fn broadcast_tx(&self, tx: String) -> Result<(), StateError> {
        Ok(self.0.broadcast_tx(context::current(), tx).await??)
    }

    pub async fn get_snarkos_block_lite(
        &self,
        block_hash: String,
    ) -> Result<Option<SnarkOSLiteBlock>, StateError> {
        Ok(self
            .0
            .get_snarkos_block_lite(context::current(), block_hash)
            .await??)
    }

    pub async fn find_transaction(&self, tx_id: String) -> Result<Option<String>, StateError> {
        Ok(self.0.find_transaction(context::current(), tx_id).await??)
    }
}
