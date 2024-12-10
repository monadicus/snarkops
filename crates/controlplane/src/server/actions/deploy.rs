use std::sync::Arc;

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use http::StatusCode;
use snops_common::{
    action_models::DeployAction,
    aot_cmds::AotCmd,
    state::{id_or_none, Authorization, KeyState},
};

use super::{execute::execute_status, Env};
use crate::{
    cannon::{error::AuthorizeError, router::AuthQuery},
    env::{error::ExecutionError, Environment},
    server::error::ServerError,
    state::GlobalState,
    unwrap_or_bad_request,
};

pub async fn deploy(
    State(state): State<Arc<GlobalState>>,
    Env { env, .. }: Env,
    Query(query): Query<AuthQuery>,
    Json(action): Json<DeployAction>,
) -> Response {
    let cannon_id = unwrap_or_bad_request!("invalid cannon id", id_or_none(&action.cannon));
    let query_addr = env.cannons.get(&cannon_id).map(|c| c.get_local_query());

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

    let Some(cannon_id) = id_or_none(&cannon_id) else {
        return Err(ExecutionError::UnknownCannon(cannon_id));
    };

    let Some(cannon) = env.cannons.get(&cannon_id) else {
        return Err(ExecutionError::UnknownCannon(cannon_id.to_string()));
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
    let mut auth_str = aot
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
