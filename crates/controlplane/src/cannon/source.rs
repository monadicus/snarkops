use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Value};
use snops_common::events::{EventHelpers, TransactionEvent};
use snops_common::schema::cannon::source::{ComputeTarget, LocalService, QueryTarget, TxSource};
use snops_common::state::NetworkId;
use snops_common::state::{Authorization, TransactionSendState};
use tracing::error;

use super::context::CtxEventHelper;
use super::{
    error::{CannonError, SourceError},
    net::get_available_port,
    tracker::TransactionTracker,
    ExecutionContext,
};
use crate::env::set::find_compute_agent;
use crate::state::EmitEvent;

pub trait GetStateRoot {
    fn get_state_root(
        &self,
        network: NetworkId,
        port: u16,
    ) -> impl std::future::Future<Output = Result<String, CannonError>>;
}

impl GetStateRoot for LocalService {
    // TODO: cache this when sync_from is false
    /// Fetch the state root from the local query service
    /// (non-cached)
    async fn get_state_root(&self, network: NetworkId, port: u16) -> Result<String, CannonError> {
        let url = format!("http://127.0.0.1:{port}/{network}/latest/stateRoot");
        let response = reqwest::get(&url)
            .await
            .map_err(|e| SourceError::FailedToGetStateRoot(url, e))?;
        Ok(response
            .json()
            .await
            .map_err(SourceError::StateRootInvalidJson)?)
    }
}

pub trait GetQueryPort {
    fn get_query_port(&self) -> Result<Option<u16>, CannonError>;
}

impl GetQueryPort for TxSource {
    /// Get an available port for the query service if applicable
    fn get_query_port(&self) -> Result<Option<u16>, CannonError> {
        if !matches!(self.query, QueryTarget::Local(_)) {
            return Ok(None);
        }
        Ok(Some(
            get_available_port().ok_or(SourceError::TxSourceUnavailablePort)?,
        ))
    }
}

pub trait ExecuteAuth {
    /// Execute the authorization and emit it to the transaction tracker
    fn execute(
        &self,
        ctx: &ExecutionContext,
        query_path: &str,
        tx_id: &Arc<String>,
        auth: &Authorization,
    ) -> impl std::future::Future<Output = Result<(), CannonError>>;
}

impl ExecuteAuth for ComputeTarget {
    async fn execute(
        self: &ComputeTarget,
        ctx: &ExecutionContext,
        query_path: &str,
        tx_id: &Arc<String>,
        auth: &Authorization,
    ) -> Result<(), CannonError> {
        match self {
            ComputeTarget::Agent { labels } => {
                // find a client, mark it as busy
                let (agent_id, client, _busy) =
                    find_compute_agent(&ctx.state, &labels.clone().unwrap_or_default())
                        .ok_or(SourceError::NoAvailableAgents("authorization"))?;

                // emit status updates & increment attempts
                TransactionEvent::Executing
                    .with_cannon_ctx(ctx, Arc::clone(tx_id))
                    .with_agent_id(agent_id)
                    .emit(ctx);
                ctx.write_tx_status(tx_id, TransactionSendState::Executing(Utc::now()));
                if let Err(e) = TransactionTracker::inc_attempts(
                    &ctx.state,
                    &(ctx.env_id, ctx.id, tx_id.to_owned()),
                ) {
                    error!(
                        "cannon {}.{} failed to increment auth attempts for {tx_id}: {e}",
                        ctx.env_id, ctx.id
                    );
                }

                // execute the authorization
                let transaction_json = client
                    .execute_authorization(
                        ctx.env_id,
                        ctx.network,
                        query_path.to_owned(),
                        serde_json::to_string(&auth)
                            .map_err(|e| SourceError::Json("authorize tx", e))?,
                    )
                    .await?;

                let transaction = match serde_json::from_str::<Arc<Value>>(&transaction_json) {
                    Ok(transaction) => transaction,
                    Err(e) => {
                        TransactionEvent::ExecuteFailed(format!(
                            "failed to parse transaction JSON: {e}\n{transaction_json}"
                        ))
                        .with_cannon_ctx(ctx, Arc::clone(tx_id))
                        .with_agent_id(agent_id)
                        .emit(ctx);
                        return Err(CannonError::Source(SourceError::Json(
                            "parse compute tx",
                            e,
                        )));
                    }
                };

                // update the transaction blob and tracker status
                let key = (ctx.env_id, ctx.id, tx_id.to_owned());
                if let Some(mut tx) = ctx.transactions.get_mut(tx_id) {
                    if let Err(e) = TransactionTracker::write_status(
                        &ctx.state,
                        &key,
                        TransactionSendState::Unsent,
                    ) {
                        error!(
                            "cannon {}.{} failed to write status after auth for {tx_id}: {e}",
                            ctx.env_id, ctx.id
                        );
                    }
                    if let Err(e) = TransactionTracker::write_tx(&ctx.state, &key, &transaction) {
                        error!(
                            "cannon {}.{} failed to write tx json after auth for {tx_id}: {e}",
                            ctx.env_id, ctx.id
                        );
                    }

                    // clear auth attempts so the broadcast has a clean slate
                    if let Err(e) = TransactionTracker::clear_attempts(
                        &ctx.state,
                        &(ctx.env_id, ctx.id, tx_id.to_owned()),
                    ) {
                        tracing::error!(
                            "cannon {}.{} failed to clear auth attempts for {tx_id}: {e}",
                            ctx.env_id,
                            ctx.id
                        );
                    }
                    tx.status = TransactionSendState::Unsent;
                    tx.transaction = Some(Arc::clone(&transaction));
                }
                TransactionEvent::ExecuteComplete {
                    transaction: Arc::clone(&transaction),
                }
                .with_cannon_ctx(ctx, Arc::clone(tx_id))
                .with_agent_id(agent_id)
                .emit(ctx);

                Ok(())
            }
            ComputeTarget::Demox { demox_api: url } => match auth {
                Authorization::Program { auth, fee_auth } => {
                    let _body = json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "generateTransaction",
                        "params": {
                            "authorization": serde_json::to_string(&auth).map_err(|e| SourceError::Json("authorize tx", e))?,
                            "fee": serde_json::to_string(&fee_auth).map_err(|e| SourceError::Json("authorize fee", e))?,
                            "url": query_path,
                            "broadcast": true,
                        }
                    });

                    todo!("post on {url}")
                }
                Authorization::Deploy {
                    owner: _,
                    deployment: _,
                    fee_auth: _,
                } => {
                    unimplemented!()
                }
            },
        }
    }
}
