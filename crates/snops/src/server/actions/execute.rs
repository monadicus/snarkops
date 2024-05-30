use axum::{
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use snops_common::{
    action_models::{AleoValue, ExecuteAction},
    aot_cmds::AotCmd,
    state::KeyState,
};
use tokio::{select, sync::mpsc};

use super::Env;
use crate::{
    cannon::{
        error::AuthorizeError,
        status::{TransactionStatus, TransactionStatusSender},
        Authorization,
    },
    env::{error::ExecutionError, Environment},
    server::error::{ActionError, ServerError},
};

pub async fn execute_status(
    tx_id: String,
    mut rx: mpsc::Receiver<TransactionStatus>,
) -> Result<Json<serde_json::Value>, ActionError> {
    use TransactionStatus::*;

    let mut timeout = Box::pin(tokio::time::sleep(std::time::Duration::from_secs(10)));
    let mut agent_id = None;
    let mut retries = 0;

    loop {
        select! {
            _ = &mut timeout => {
                return Err(ActionError::ExecuteStatusTimeout { tx_id, agent_id, retries });
            },
            Some(msg) = rx.recv() => {
                match msg {
                    ExecuteAborted => {
                        return Err(ActionError::ExecuteStatusAborted { tx_id, retries});
                    },
                    ExecuteFailed(msg) => {
                        return Err(ActionError::ExecuteStatusFailed { message: msg, tx_id, retries });
                    },
                    Executing(id) => {
                        agent_id = Some(id.to_string());
                    },
                    ExecuteAwaitingCompute => {
                        retries += 1;
                    },
                    ExecuteComplete => {
                        return Ok(Json(json!({
                            "transaction_id": tx_id,
                            "agent_id": agent_id,
                            "retries": retries
                        })));
                    },
                    _ => (),
                }
            },
        }
    }
}

pub async fn execute(Env { env, .. }: Env, Json(action): Json<ExecuteAction>) -> Response {
    let (tx, rx) = mpsc::channel(10);

    let tx_id = match execute_inner(action, &env, TransactionStatusSender::new(tx)).await {
        Ok(tx_id) => tx_id,
        Err(e) => return ServerError::from(e).into_response(),
    };

    execute_status(tx_id, rx).await.into_response()
}

pub async fn execute_inner(
    action: ExecuteAction,
    env: &Environment,
    events: TransactionStatusSender,
) -> Result<String, ExecutionError> {
    let ExecuteAction {
        cannon: cannon_id,
        private_key,
        program,
        function,
        inputs,
        priority_fee,
        fee_record,
    } = action;

    let Some(cannon) = env.cannons.get(&cannon_id) else {
        return Err(ExecutionError::UnknownCannon(cannon_id));
    };

    let KeyState::Literal(resolved_pk) = env.storage.sample_keysource_pk(&private_key) else {
        return Err(AuthorizeError::MissingPrivateKey(
            format!("{}.{cannon_id} {program}/{function}", env.id),
            private_key.to_string(),
        )
        .into());
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
    let aot = AotCmd::new(env.aot_bin.clone(), env.network);
    let auth_str = aot
        .authorize(
            &resolved_pk,
            &program,
            &function,
            &resolved_inputs,
            priority_fee,
            fee_record.as_ref(),
        )
        .await?;

    // parse the json and bundle it up
    let authorization: Authorization =
        serde_json::from_str(&auth_str).map_err(AuthorizeError::Json)?;

    let tx_id = authorization.get_tx_id(&aot).await?;

    // proxy it to a listen cannon
    cannon.proxy_auth(authorization, events)?;

    Ok(tx_id)
}
