use axum::{
    response::{IntoResponse, Response},
    Json,
};
use snops_common::{
    action_models::DeployAction,
    aot_cmds::{AotCmd, Authorization},
    state::KeyState,
};
use tokio::sync::mpsc;

use super::{execute::execute_status, Env};
use crate::{
    cannon::{error::AuthorizeError, status::TransactionStatusSender},
    env::{error::ExecutionError, Environment},
    server::error::ServerError,
};

pub async fn deploy(Env { env, .. }: Env, Json(action): Json<DeployAction>) -> Response {
    let (tx, rx) = mpsc::channel(10);

    let query = env.cannons.get(&action.cannon).map(|c| c.get_local_query());

    match deploy_inner(action, &env, TransactionStatusSender::new(tx), query).await {
        Ok(tx_id) => execute_status(tx_id, rx).await.into_response(),
        Err(e) => ServerError::from(e).into_response(),
    }
}

pub async fn deploy_inner(
    action: DeployAction,
    env: &Environment,
    events: TransactionStatusSender,
    query: Option<String>,
) -> Result<String, ExecutionError> {
    let DeployAction {
        cannon: cannon_id,
        private_key,
        fee_private_key,
        program,
        priority_fee,
        fee_record,
    } = action;

    let Some(cannon) = env.cannons.get(&cannon_id) else {
        return Err(ExecutionError::UnknownCannon(cannon_id));
    };

    let KeyState::Literal(resolved_pk) = env.storage.sample_keysource_pk(&private_key) else {
        return Err(AuthorizeError::MissingPrivateKey(
            format!("{}.{cannon_id} deployed program", env.id),
            private_key.to_string(),
        )
        .into());
    };

    let resolved_fee_pk = if let Some(fee_key) = fee_private_key {
        let KeyState::Literal(pk) = env.storage.sample_keysource_pk(&fee_key) else {
            return Err(AuthorizeError::MissingPrivateKey(
                format!("{}.{cannon_id} deployed program", env.id),
                fee_key.to_string(),
            )
            .into());
        };
        Some(pk)
    } else {
        None
    };

    // authorize the transaction
    let aot = AotCmd::new(env.aot_bin.clone(), env.network);
    let auth_str = aot
        .authorize_deploy(
            &resolved_pk,
            resolved_fee_pk.as_ref(),
            &program,
            query.as_ref(),
            priority_fee,
            fee_record.as_ref(),
        )
        .await?;

    // parse the json and bundle it up
    let authorization: Authorization =
        serde_json::from_str(&auth_str).map_err(AuthorizeError::Json)?;

    let tx_id = aot.get_tx_id(&authorization).await?;

    // proxy it to a listen cannon
    cannon.proxy_auth(authorization, events)?;

    Ok(tx_id)
}
