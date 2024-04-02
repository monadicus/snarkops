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

use super::{error::ServerError, AppState};
use crate::cannon::router::redirect_cannon_routes;
use crate::env::Environment;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/agents", get(get_agents))
        .route("/agents/:id/tps", get(get_agent_tps))
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

fn status_ok() -> Response {
    (StatusCode::OK, Json(json!({"status": "ok"}))).into_response()
}

async fn get_agent_tps(state: State<AppState>, Path(id): Path<AgentId>) -> Response {
    let pool = state.pool.read().await;
    let Some(agent) = pool.get(&id) else {
        return ServerError::AgentNotFound(id).into_response();
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
        Err(e) => return ServerError::from(e).into_response(),
    };

    // TODO: some live state to report to the calling CLI or something would be
    // really nice

    // TODO: clean up existing test

    match Environment::prepare(documents, &state).await {
        Ok(env_id) => (StatusCode::OK, Json(json!({ "id": env_id }))).into_response(),
        Err(e) => ServerError::from(e).into_response(),
    }
}

async fn post_env_timeline(Path(env_id): Path<usize>, State(state): State<AppState>) -> Response {
    match Environment::execute(state, env_id).await {
        Ok(()) => status_ok(),
        Err(e) => ServerError::from(e).into_response(),
    }
}

async fn delete_env_timeline(
    Path(env_id): Path<usize>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match Environment::cleanup(&env_id, &state).await {
        Ok(_) => status_ok(),
        Err(e) => ServerError::from(e).into_response(),
    }
}
