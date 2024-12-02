use std::sync::Arc;

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use http::StatusCode;
use snops_common::{
    action_models::DeployAction,
    aot_cmds::{AotCmd, Authorization},
    state::KeyState,
};

use super::{execute::execute_status, Env};
use crate::{
    cannon::{error::AuthorizeError, router::AuthQuery},
    env::{error::ExecutionError, Environment},
    server::error::ServerError,
    state::GlobalState,
};

pub async fn deploy(
    State(state): State<Arc<GlobalState>>,
    Env { env, .. }: Env,
    Query(query): Query<AuthQuery>,
    Json(action): Json<DeployAction>,
) -> Response {
    let query_addr = env.cannons.get(&action.cannon).map(|c| c.get_local_query());
    let cannon_id = action.cannon;

    if query.is_async() {
        return match deploy_inner(&state, action, &env, query_addr).await {
            Ok(tx_id) => (StatusCode::ACCEPTED, Json(tx_id)).into_response(),
            Err(e) => ServerError::from(e).into_response(),
        };
    }

    match deploy_inner(&state, action, &env, query_addr).await {
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

pub async fn deploy_inner(
    state: &GlobalState,
    action: DeployAction,
    env: &Environment,
    query: Option<String>,
) -> Result<Arc<String>, ExecutionError> {
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

    let compute_bin = env.storage.resolve_compute_binary(state).await?;
    // authorize the transaction
    let aot = AotCmd::new(compute_bin, env.network);
    let auth_str = aot
        .authorize_deploy(
            &resolved_pk,
            resolved_fee_pk.as_ref(),
            &program,
            query.as_ref(),
            priority_fee,
            fee_record.as_ref(),
            // use cost_v1 when we are not using the native genesis
            !env.storage.native_genesis,
        )
        .await?;

    // parse the json and bundle it up
    let authorization: Authorization =
        serde_json::from_str(&auth_str).map_err(AuthorizeError::Json)?;

    // proxy it to a listen cannon
    let tx_id = cannon.proxy_auth(authorization).await?;

    Ok(tx_id)
}
