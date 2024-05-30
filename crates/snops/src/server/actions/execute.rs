use axum::{
    response::{IntoResponse, Response},
    Json,
};
use snops_common::{aot_cmds::AotCmd, state::KeyState};
use tokio::{select, sync::mpsc};

use super::{
    models::{AleoValue, ExecuteAction},
    Env,
};
use crate::{
    cannon::{
        error::AuthorizeError,
        status::{TransactionStatus, TransactionStatusSender},
        Authorization,
    },
    env::{error::ExecutionError, Environment},
    json_response,
    server::error::ServerError,
};

pub async fn execute_status(tx_id: String, mut rx: mpsc::Receiver<TransactionStatus>) -> Response {
    use TransactionStatus::*;

    let mut timeout = Box::pin(tokio::time::sleep(std::time::Duration::from_secs(10)));
    let mut agent_id = None;
    let mut retries = 0;

    loop {
        select! {
            _ = &mut timeout => {
                return json_response!(REQUEST_TIMEOUT, {
                    "error": "execution timed out",
                    "transaction_id": tx_id,
                    "agent_id": agent_id,
                    "retries": retries
                });
            },
            Some(msg) = rx.recv() => {
                match msg {
                    ExecuteAborted => {
                        return json_response!(INTERNAL_SERVER_ERROR, {
                            "error": "execution aborted",
                            "transaction_id": tx_id,
                            "retries": retries
                        });
                    },
                    ExecuteFailed(msg) => {
                        return json_response!(INTERNAL_SERVER_ERROR, {
                            "error": "execution failed",
                            "message": msg,
                            "transaction_id": tx_id,
                            "retries": retries
                        });
                    },
                    Executing(id) => {
                        agent_id = Some(id);
                    },
                    ExecuteAwaitingCompute => {
                        retries += 1;
                    },
                    ExecuteComplete => {
                        return json_response!(OK, {
                            "transaction_id": tx_id,
                            "agent_id": agent_id,
                            "retries": retries
                        });
                    },
                    _ => (),
                }
            },
        }
    }
}

pub async fn execute(Env { env, .. }: Env, Json(action): Json<ExecuteAction>) -> Response {
    let (tx, rx) = mpsc::channel(10);

    let tx_id = match action.execute(&env, TransactionStatusSender::new(tx)).await {
        Ok(tx_id) => tx_id,
        Err(e) => return ServerError::from(e).into_response(),
    };

    execute_status(tx_id, rx).await
}

impl ExecuteAction {
    pub async fn execute(
        &self,
        env: &Environment,
        events: TransactionStatusSender,
    ) -> Result<String, ExecutionError> {
        let Self {
            cannon: cannon_id,
            private_key,
            program,
            function,
            inputs,
            priority_fee,
            fee_record,
        } = &self;

        let Some(cannon) = env.cannons.get(cannon_id) else {
            return Err(ExecutionError::UnknownCannon(*cannon_id));
        };

        let KeyState::Literal(resolved_pk) = env.storage.sample_keysource_pk(private_key) else {
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
                program,
                function,
                &resolved_inputs,
                *priority_fee,
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
}
