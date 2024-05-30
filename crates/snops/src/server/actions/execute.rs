use axum::{
    response::{IntoResponse, Response},
    Json,
};
use snops_common::{aot_cmds::AotCmd, state::KeyState};

use super::{
    models::{AleoValue, ExecuteAction},
    Env,
};
use crate::{
    cannon::{error::AuthorizeError, Authorization},
    env::{error::ExecutionError, Environment},
    server::error::ServerError,
};

pub async fn execute(Env { env, .. }: Env, Json(action): Json<ExecuteAction>) -> Response {
    action
        .execute(&env)
        .await
        .map_err(ServerError::from)
        .into_response()
}

impl ExecuteAction {
    pub async fn execute(&self, env: &Environment) -> Result<String, ExecutionError> {
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

        let tx_id = aot
            .get_tx_id(
                serde_json::to_string(&authorization.auth).map_err(AuthorizeError::Json)?,
                authorization
                    .fee_auth
                    .as_ref()
                    .map(|fee_auth| serde_json::to_string(&fee_auth).map_err(AuthorizeError::Json))
                    .transpose()?,
            )
            .await?;

        // proxy it to a listen cannon
        cannon.proxy_auth(authorization)?;

        Ok(tx_id)
    }
}
