use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use snot_common::{rpc::agent::AgentMetric, state::AgentId};

use super::AppState;
use crate::cannon::router::redirect_cannon_routes;
use crate::env::Environment;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/agents", get(get_agents))
        .route("/agents/:id/tps", get(get_agent_tps))
        .route("/agents/:id/metrics", get(get_agent_metrics))
        .route("/env/prepare", post(post_env_prepare))
        .route("/env/:env_id/storage/:ty", get(redirect_storage))
        .nest("/env/:env_id/cannons", redirect_cannon_routes())
        .route("/env/:id", post(post_env_timeline))
        .route("/env/:id", delete(delete_env_timeline))
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum StorageType {
    Genesis,
    Ledger,
}

async fn redirect_storage(
    Path((env_id, ty)): Path<(usize, StorageType)>,
    state: State<AppState>,
) -> Response {
    let Some(env) = state.envs.read().await.get(&env_id).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let real_id = &env.storage.id;

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

async fn get_agent_metrics(state: State<AppState>, Path(id): Path<AgentId>) -> Response {
    let client = {
        let pool = state.pool.read().await;
        let Some(agent) = pool.get(&id) else {
            return StatusCode::NOT_FOUND.into_response();
        };

        let Some(client) = agent.client_owned() else {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "agent is not an active node"})),
            )
                .into_response();
        };

        client
    };

    match client
        .into_inner()
        .get_metrics(tarpc::context::current())
        .await
    {
        Ok(Ok(body)) => body.into_response(),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "could not fetch metrics"})),
        )
            .into_response(),
    }
}

async fn get_agent_tps(state: State<AppState>, Path(id): Path<AgentId>) -> Response {
    let pool = state.pool.read().await;
    let Some(agent) = pool.get(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // TODO: get rid of these unwraps
    agent
        .rpc()
        .unwrap()
        .get_metric(tarpc::context::current(), AgentMetric::Tps)
        .await
        .unwrap()
        .to_string()
        .into_response()
}

async fn post_env_prepare(state: State<AppState>, body: String) -> Response {
    let documents = match Environment::deserialize(&body) {
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

    match Environment::prepare(documents, &state).await {
        Ok(env_id) => (StatusCode::OK, Json(json!({ "id": env_id }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

async fn post_env_timeline(Path(env_id): Path<usize>, State(state): State<AppState>) -> Response {
    match Environment::execute(state, env_id).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

async fn delete_env_timeline(
    Path(env_id): Path<usize>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match Environment::cleanup(&env_id, &state).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}
