use std::time::Duration;

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use reqwest::StatusCode;
use serde_json::json;
use snops_common::{
    aot_cmds::AotCmd,
    state::{id_or_none, NetworkId},
};
use tokio::sync::mpsc;

use super::{status::TransactionStatusSender, Authorization};
use crate::{server::actions::execute::execute_status, state::AppState};

pub(crate) fn redirect_cannon_routes() -> Router<AppState> {
    Router::new()
        .route("/:cannon/:network/latest/stateRoot", get(state_root))
        .route("/:cannon/:network/transaction/broadcast", post(transaction))
        .route("/:cannon/auth", post(authorization))
}

async fn state_root(
    Path((env_id, cannon_id, network)): Path<(String, String, NetworkId)>,
    state: State<AppState>,
) -> Response {
    let (Some(env_id), Some(cannon_id)) = (id_or_none(&env_id), id_or_none(&cannon_id)) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "unknown cannon or environment" })),
        )
            .into_response();
    };

    let Some(env) = state.get_env(env_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "environment not found" })),
        )
            .into_response();
    };

    if env.network != network {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "network mismatch" })),
        )
            .into_response();
    }

    let Some(cannon) = env.get_cannon(cannon_id) else {
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
    Path((env_id, cannon_id, network)): Path<(String, String, NetworkId)>,
    state: State<AppState>,
    body: String,
) -> Response {
    let (Some(env_id), Some(cannon_id)) = (id_or_none(&env_id), id_or_none(&cannon_id)) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "unknown cannon or environment" })),
        )
            .into_response();
    };

    let Some(env) = state.get_env(env_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "environment not found" })),
        )
            .into_response();
    };

    if env.network != network {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "network mismatch" })),
        )
            .into_response();
    }

    let Some(cannon) = env.get_cannon(cannon_id) else {
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
    Path((env_id, cannon_id)): Path<(String, String)>,
    state: State<AppState>,
    Json(body): Json<Authorization>,
) -> Response {
    let (Some(env_id), Some(cannon_id)) = (id_or_none(&env_id), id_or_none(&cannon_id)) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "unknown cannon or environment" })),
        )
            .into_response();
    };

    let Some(env) = state.get_env(env_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "environment not found" })),
        )
            .into_response();
    };

    let Some(cannon) = env.get_cannon(cannon_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "cannon not found" })),
        )
            .into_response();
    };

    let aot = AotCmd::new(env.aot_bin.clone(), env.network);
    let tx_id = match body.get_tx_id(&aot).await {
        Ok(id) => id,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("{e}")})),
            )
                .into_response()
        }
    };

    let (tx, rx) = mpsc::channel(10);

    match cannon.proxy_auth(body, TransactionStatusSender::new(tx)) {
        Ok(_) => execute_status(tx_id, rx).await,
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}")})),
        )
            .into_response(),
    }
}
