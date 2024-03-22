use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;

use super::AppState;
use crate::testing::Test;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/storage/:id/:ty", get(redirect_storage))
        .route("/agents", get(get_agents))
        .route("/test/prepare", post(post_test_prepare))
        .route("/test", delete(delete_test))
}

#[derive(Deserialize)]
enum StorageType {
    Genesis,
    Ledger,
}

async fn redirect_storage(
    Path((storage_id, ty)): Path<(usize, StorageType)>,
    State(state): State<AppState>,
) -> Response {
    let Some(real_id) = state.storage.read().await.get_by_left(&storage_id).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let filename = match ty {
        StorageType::Genesis => "genesis.block",
        StorageType::Ledger => "ledger.tar.gz",
    };

    Redirect::temporary(&format!("/content/storage/{real_id}/{filename}")).into_response()
}

async fn get_agents(State(state): State<AppState>) -> impl IntoResponse {
    // TODO: return actual relevant info about agents
    Json(json!({ "count": state.pool.read().await.len() }))
}

async fn post_test_prepare(State(state): State<AppState>, body: String) -> Response {
    let Ok(documents) = Test::deserialize(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    // TODO: some live state to report to the calling CLI or something would be
    // really nice

    // TODO: clean up existing test

    match Test::prepare(documents, &state).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

async fn delete_test(State(state): State<AppState>) -> impl IntoResponse {
    match Test::cleanup(&state).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
