use std::sync::Arc;

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use http::StatusCode;
use serde_json::json;
use snops_common::{
    action_models::{AleoValue, ExecuteAction},
    aot_cmds::AotCmd,
    events::{Event, EventKind},
    state::{id_or_none, Authorization, KeyState},
};
use tokio::select;

use super::Env;
use crate::{
    cannon::{error::AuthorizeError, router::AuthQuery},
    env::{error::ExecutionError, Environment},
    events::EventSubscriber,
    server::error::{ActionError, ServerError},
    state::GlobalState,
};

pub async fn execute_status(
    tx_id: Arc<String>,
    mut rx: EventSubscriber,
) -> Result<Json<serde_json::Value>, ActionError> {
    use snops_common::events::TransactionEvent::*;

    let mut timeout = Box::pin(tokio::time::sleep(std::time::Duration::from_secs(30)));
    let mut agent_id = None;
    let mut retries = 0;

    loop {
        select! {
            _ = &mut timeout => {
                return Err(ActionError::ExecuteStatusTimeout { tx_id: tx_id.to_string(), agent_id, retries });
            },
            Ok(ev) = rx.next() => {
                let Event{ content: EventKind::Transaction(ev), agent, .. } = ev.as_ref() else {
                    continue;
                };

                match ev {
                    ExecuteAborted(reason) => {
                        return Err(ActionError::ExecuteStatusAborted {
                            tx_id: tx_id.to_string(),
                            retries,
                            reason: reason.clone(),
                        });
                    },
                    ExecuteFailed(message) => {
                        return Err(ActionError::ExecuteStatusFailed {
                            message: message.to_string(),
                            tx_id: tx_id.to_string(),
                            retries,
                        });
                    },
                    Executing => {
                        agent_id = agent.map(|id| id.to_string());
                    },
                    ExecuteAwaitingCompute => {
                        retries += 1;
                    },
                    ExecuteComplete { transaction } => {
                        return Ok(Json(json!({
                            "agent_id": agent_id,
                            "retries": retries,
                            "transaction": transaction,
                        })));
                    },
                    _ => (),
                }
            },
        }
    }
}

pub async fn execute(
    State(state): State<Arc<GlobalState>>,
    Env { env, .. }: Env,
    Query(query): Query<AuthQuery>,
    Json(action): Json<ExecuteAction>,
) -> Response {
    let Some(cannon_id) = id_or_none(&action.cannon) else {
        return ServerError::from(ExecutionError::UnknownCannon(action.cannon)).into_response();
    };
    let query_addr = env.cannons.get(&cannon_id).map(|c| c.get_local_query());

    if query.is_async() {
        return match execute_inner(&state, action, &env, query_addr).await {
            Ok(tx_id) => (StatusCode::ACCEPTED, Json(tx_id)).into_response(),
            Err(e) => ServerError::from(e).into_response(),
        };
    }

    match execute_inner(&state, action, &env, query_addr).await {
        Ok(tx_id) => {
            use snops_common::events::EventFilter::*;
            let subscriber = state
                .events
                .subscribe_on(TransactionIs(tx_id.clone()) & EnvIs(env.id) & CannonIs(cannon_id));
            execute_status(tx_id, subscriber).await.into_response()
        }
        Err(e) => ServerError::from(e).into_response(),
    }
}

pub async fn execute_inner(
    state: &GlobalState,
    action: ExecuteAction,
    env: &Environment,
    query: Option<String>,
) -> Result<Arc<String>, ExecutionError> {
    let ExecuteAction {
        cannon: cannon_id,
        private_key,
        fee_private_key,
        program,
        function,
        inputs,
        priority_fee,
        fee_record,
    } = action;
    let Some(cannon_id) = id_or_none(&cannon_id) else {
        return Err(ExecutionError::UnknownCannon(cannon_id));
    };

    let Some(cannon) = env.cannons.get(&cannon_id) else {
        return Err(ExecutionError::UnknownCannon(cannon_id.to_string()));
    };

    let KeyState::Literal(resolved_pk) = env.storage.sample_keysource_pk(&private_key) else {
        return Err(AuthorizeError::MissingPrivateKey(
            format!("{}.{cannon_id} {program}/{function}", env.id),
            private_key.to_string(),
        )
        .into());
    };

    let resolved_fee_pk = if let Some(fee_key) = fee_private_key {
        let KeyState::Literal(pk) = env.storage.sample_keysource_pk(&fee_key) else {
            return Err(AuthorizeError::MissingPrivateKey(
                format!("{}.{cannon_id} {program}/{function}", env.id),
                fee_key.to_string(),
            )
            .into());
        };
        Some(pk)
    } else {
        None
    };

    let resolved_inputs = inputs
        .iter()
        .map(|input| match input {
            AleoValue::Key(key) => match env.storage.sample_keysource_addr(key) {
                KeyState::Literal(key) => Ok(key),
                _ => Err(AuthorizeError::InvalidProgramInputs(
                    format!("{program}/{function}"),
                    format!("key {key} does not resolve a valid addr"),
                )),
            },
            AleoValue::Other(value) => Ok(value.clone()),
        })
        .collect::<Result<Vec<String>, AuthorizeError>>()?;

    // authorize the transaction
    let compute_bin = env.storage.resolve_compute_binary(state).await?;
    let aot = AotCmd::new(compute_bin, env.network);
    let mut auth_str = aot
        .authorize_program(
            &resolved_pk,
            resolved_fee_pk.as_ref(),
            &program,
            &function,
            &resolved_inputs,
            query.as_ref(),
            priority_fee,
            fee_record.as_ref(),
            // use cost_v1 when we are not using the native genesis
            !env.storage.native_genesis,
        )
        .await?;

    // Truncate the output to the first {
    // because Aleo decided to print execute
    // status to stdout...
    if let Some(index) = auth_str.find("{") {
        auth_str = auth_str.split_off(index);
    }

    // parse the json and bundle it up
    let authorization: Authorization = serde_json::from_str(&auth_str)
        .inspect_err(|e| {
            tracing::error!("failed to parse authorization json: {e}");
            tracing::error!("authorization json: {auth_str}");
        })
        .map_err(AuthorizeError::Json)?;

    // proxy it to a listen cannon
    let tx_id = cannon.proxy_auth(authorization).await?;

    Ok(tx_id)
}
