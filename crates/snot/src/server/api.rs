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
        .route("/test/:id", delete(delete_test))
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum StorageType {
    Genesis,
    Ledger,
}

async fn redirect_storage(
    Path((storage_id, ty)): Path<(usize, StorageType)>,
    state: State<AppState>,
) -> Response {
    let Some(real_id) = state
        .storage_ids
        .read()
        .await
        .get_by_left(&storage_id)
        .cloned()
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let filename = match ty {
        StorageType::Genesis => "genesis.block",
        StorageType::Ledger => "ledger.tar.gz",
    };

    Redirect::temporary(&format!("/content/storage/{real_id}/{filename}")).into_response()
}

async fn get_agents(state: State<AppState>) -> impl IntoResponse {
    // TODO: return actual relevant info about agents
    Json(json!({ "count": state.pool.read().await.len() }))
}

async fn post_test_prepare(state: State<AppState>, body: String) -> Response {
    let documents = match Test::deserialize(&body) {
        Ok(documents) => documents,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("{e}")})),
            )
                .into_response();
        }
    };

    // TODO: some live state to report to the calling CLI or something would be
    // really nice

    // TODO: clean up existing test

    // TODO: support concurrent tests + return test id

    match Test::prepare(documents, &state).await {
        Ok(test_id) => (StatusCode::OK, Json(json!({ "id": test_id }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

async fn delete_test(
    Path(test_id): Path<usize>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match Test::cleanup(&test_id, &state).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}
