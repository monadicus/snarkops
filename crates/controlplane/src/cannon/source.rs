use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use snops_common::events::{EventHelpers, TransactionEvent};
use snops_common::state::{Authorization, TransactionSendState};
use snops_common::{INTERN, lasso::Spur, node_targets::NodeTargets, state::NetworkId};
use tracing::error;

use super::context::CtxEventHelper;
use super::{
    ExecutionContext,
    error::{CannonError, SourceError},
    net::get_available_port,
    tracker::TransactionTracker,
};
use crate::env::set::find_compute_agent;
use crate::state::EmitEvent;

/// Represents an instance of a local query service.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalService {
    // TODO debate this
    /// An optional node to sync blocks from...
    /// necessary for private tx mode in realtime mode as this will have to
    /// sync from a node that has a valid ledger
    ///
    /// When present, the cannon will update the ledger service from this node
    /// if the node is out of sync, it will corrupt the ledger...
    ///
    /// requires cannon to have an associated env_id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_from: Option<NodeTargets>,
}

impl LocalService {
    // TODO: cache this when sync_from is false
    /// Fetch the state root from the local query service
    /// (non-cached)
    pub async fn get_state_root(
        &self,
        network: NetworkId,
        port: u16,
    ) -> Result<String, CannonError> {
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

/// Used to determine the redirection for the following paths:
/// /cannon/<id>/<network>/latest/stateRoot
/// /cannon/<id>/<network>/transaction/broadcast
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", untagged)]
pub enum QueryTarget {
    /// Target a specific node (probably over rpc instead of reqwest lol...)
    ///
    /// Requires cannon to have an associated env_id
    Node(NodeTargets),
    /// Use the local ledger query service
    Local(LocalService),
}

impl Default for QueryTarget {
    fn default() -> Self {
        QueryTarget::Local(LocalService { sync_from: None })
    }
}

fn deser_labels<'de, D>(deser: D) -> Result<Option<Vec<Spur>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<String>>::deserialize(deser)?.map(|s| {
        s.into_iter()
            .map(|s| INTERN.get_or_intern(s))
            .collect::<Vec<Spur>>()
    }))
}

fn ser_labels<S>(labels: &Option<Vec<Spur>>, ser: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match labels {
        Some(labels) => {
            let labels = labels
                .iter()
                .map(|s| INTERN.resolve(s))
                .collect::<Vec<&str>>();
            serde::Serialize::serialize(&labels, ser)
        }
        None => serde::Serialize::serialize(&None::<String>, ser),
    }
}

/// Which service is providing the compute power for executing transactions
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", untagged)]
pub enum ComputeTarget {
    /// Use the agent pool to generate executions
    Agent {
        #[serde(
            default,
            deserialize_with = "deser_labels",
            serialize_with = "ser_labels",
            skip_serializing_if = "Option::is_none"
        )]
        labels: Option<Vec<Spur>>,
    },
    /// Use demox' API to generate executions
    #[serde(rename_all = "kebab-case")]
    Demox { demox_api: String },
}

impl Default for ComputeTarget {
    fn default() -> Self {
        ComputeTarget::Agent { labels: None }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TxSource {
    /// Receive authorizations from a persistent path
    /// /api/v1/env/:env_id/cannons/:id/auth
    #[serde(default)]
    pub query: QueryTarget,
    #[serde(default)]
    pub compute: ComputeTarget,
}

impl TxSource {
    /// Get an available port for the query service if applicable
    pub fn get_query_port(&self) -> Result<Option<u16>, CannonError> {
        if !matches!(self.query, QueryTarget::Local(_)) {
            return Ok(None);
        }
        Ok(Some(
            get_available_port().ok_or(SourceError::TxSourceUnavailablePort)?,
        ))
    }
}

impl ComputeTarget {
    pub async fn execute(
        &self,
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
