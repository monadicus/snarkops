use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use reqwest::StatusCode;
use serde_json::json;

use crate::state::AppState;

pub(crate) fn redirect_cannon_routes() -> Router<AppState> {
    Router::new()
        .route("/:id/mainnet/latest/stateRoot", get(state_root))
        .route("/:id/mainnet/transaction/broadcast", post(transaction))
}

async fn state_root(Path(cannon_id): Path<usize>, state: State<AppState>) -> Response {
    let Some(cannon) = ({
        let cannons = state.cannons.read().await;
        cannons.get(&cannon_id).cloned()
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match cannon.proxy_state_root().await {
        // the nodes expect this state root to be string escaped json
        Ok(root) => Json(json!(root)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}")})),
        )
            .into_response(),
    }
}

async fn transaction(
    Path(cannon_id): Path<usize>,
    state: State<AppState>,
    body: String,
) -> Response {
    let Some(cannon) = ({
        let cannons = state.cannons.read().await;
        cannons.get(&cannon_id).cloned()
    }) else {
        return StatusCode::NOT_FOUND.into_response();
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