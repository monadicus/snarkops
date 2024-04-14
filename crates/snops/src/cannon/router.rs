use std::time::Duration;

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use reqwest::StatusCode;
use serde_json::json;
use snops_common::state::EnvId;

use super::Authorization;
use crate::state::AppState;

pub(crate) fn redirect_cannon_routes() -> Router<AppState> {
    Router::new()
        .route("/:cannon/mainnet/latest/stateRoot", get(state_root))
        .route("/:cannon/mainnet/transaction/broadcast", post(transaction))
        .route("/:cannon/auth", post(authorization))
}

async fn state_root(
    Path((env_id, cannon_id)): Path<(EnvId, usize)>,
    state: State<AppState>,
) -> Response {
    let Some(env) = ({
        let env = state.envs.read().await;
        env.get(&env_id).cloned()
    }) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "environment not found" })),
        )
            .into_response();
    };

    let cannon_lock = env.cannons.read().await;
    let Some(cannon) = cannon_lock.get(&cannon_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "cannon not found" })),
        )
            .into_response();
    };

    // TODO: lock this with a mutex or something so that multiple route callers
    // can't bombard the cannon with proxy_state_root call attempts
    let mut attempts = 0;
    loop {
        attempts += 1;
        match cannon.proxy_state_root().await {
            Ok(root) => break Json(root).into_response(),

            Err(e) if attempts > 5 => {
                break (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "non-responsive query node", "inner": format!("{e}") })),
                )
                    .into_response()
            }

            _ => attempts += 1,
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // match cannon.proxy_state_root().await {
    //     // the nodes expect this state root to be string escaped json
    //     Ok(root) => Json(root).into_response(),
    //     Err(e) => (
    //         StatusCode::INTERNAL_SERVER_ERROR,
    //         Json(json!({ "error": format!("{e}")})),
    //     )
    //         .into_response(),
    // }
}

async fn transaction(
    Path((env_id, cannon_id)): Path<(EnvId, usize)>,
    state: State<AppState>,
    body: String,
) -> Response {
    let Some(env) = ({
        let env = state.envs.read().await;
        env.get(&env_id).cloned()
    }) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "environment not found" })),
        )
            .into_response();
    };

    let cannon_lock = env.cannons.read().await;
    let Some(cannon) = cannon_lock.get(&cannon_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "cannon not found" })),
        )
            .into_response();
    };

    match cannon.proxy_broadcast(body) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}")})),
        )
            .into_response(),
    }
}

async fn authorization(
    Path((env_id, cannon_id)): Path<(EnvId, usize)>,
    state: State<AppState>,
    Json(body): Json<Authorization>,
) -> Response {
    let Some(env) = ({
        let env = state.envs.read().await;
        env.get(&env_id).cloned()
    }) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "environment not found" })),
        )
            .into_response();
    };

    let cannon_lock = env.cannons.read().await;
    let Some(cannon) = cannon_lock.get(&cannon_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "cannon not found" })),
        )
            .into_response();
    };

    match cannon.proxy_auth(body) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}")})),
        )
            .into_response(),
    }
}
