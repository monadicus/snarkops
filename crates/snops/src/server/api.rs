use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use snops_common::{
    constant::{LEDGER_STORAGE_FILE, SNARKOS_GENESIS_FILE},
    rpc::agent::AgentMetric,
    state::{id_or_none, EnvId},
};
use tower::Service;
use tower_http::services::ServeFile;
use tracing::info;

use super::{error::ServerError, AppState};
use crate::env::Environment;
use crate::{cannon::router::redirect_cannon_routes, schema::storage::DEFAULT_AOT_BIN};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/agents", get(get_agents))
        .route("/agents/:id/tps", get(get_agent_tps))
        .route("/env/list", get(get_env_list))
        .route("/env/:env_id/topology", get(get_env_topology))
        .route("/env/:env_id/prepare", post(post_env_prepare))
        .route("/env/:env_id/storage", get(get_storage_info))
        .route("/env/:env_id/storage/:ty", get(redirect_storage))
        .nest("/env/:env_id/cannons", redirect_cannon_routes())
        .route("/env/:id", delete(delete_env))
        .route("/env/:env_id/timelines/:t_id/steps", get(get_timeline))
        .route("/env/:id/timelines/:t_id", post(post_timeline))
        .route("/env/:id/timelines/:t_id", delete(delete_timeline))
        .route("/env/:env_id/timelines", get(get_timelines))
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum StorageType {
    Genesis,
    Ledger,
    Binary,
}

async fn get_storage_info(Path(env_id): Path<String>, state: State<AppState>) -> Response {
    let Some(env_id) = id_or_none(&env_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(env) = state.envs.get(&env_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    Json(env.storage.info()).into_response()
}

async fn redirect_storage(
    Path((env_id, ty)): Path<(String, StorageType)>,
    state: State<AppState>,
    req: Request,
) -> Response {
    let Some(env_id) = id_or_none(&env_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(env) = state.envs.get(&env_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let real_id = &env.storage.id;

    let filename = match ty {
        StorageType::Genesis => SNARKOS_GENESIS_FILE,
        StorageType::Ledger => LEDGER_STORAGE_FILE,
        StorageType::Binary => {
            // TODO: replace with env specific aot binary
            return ServeFile::new(DEFAULT_AOT_BIN.clone())
                .call(req)
                .await
                .into_response();
        }
    };

    Redirect::temporary(&format!("/content/storage/{real_id}/{filename}")).into_response()
}

async fn get_agents(state: State<AppState>) -> impl IntoResponse {
    // TODO: return actual relevant info about agents
    Json(json!({ "count": state.pool.len() }))
}

fn status_ok() -> Response {
    (StatusCode::OK, Json(json!({"status": "ok"}))).into_response()
}

async fn get_agent_tps(state: State<AppState>, Path(id): Path<String>) -> Response {
    let Some(agent) = id_or_none(&id).and_then(|id| state.pool.get(&id)) else {
        return ServerError::AgentNotFound(id.clone()).into_response();
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

async fn get_env_list(State(state): State<AppState>) -> Response {
    Json(state.envs.iter().map(|e| e.id).collect::<Vec<_>>()).into_response()
}

async fn get_timeline(
    Path((env_id, t_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Response {
    info!("get_env_timeline: env_id: {}, t_id: {}", env_id, t_id);
    let Some(env_id) = id_or_none(&env_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(env) = state.envs.get(&env_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(t_id) = id_or_none(&t_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(timeline) = &env.timelines.get(&t_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    Json(json!({
        "steps": timeline.len(),
    }))
    .into_response()
}

async fn get_timelines(Path(env_id): Path<String>, State(state): State<AppState>) -> Response {
    let Some(env_id) = id_or_none(&env_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(env) = state.envs.get(&env_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    Json(&env.timelines.iter().map(|t| *t.key()).collect::<Vec<_>>()).into_response()
}

async fn get_env_topology(Path(env_id): Path<String>, State(state): State<AppState>) -> Response {
    let Some(env_id) = id_or_none(dbg!(&env_id)) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(env) = state.envs.get(&env_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    Json(&env.node_map).into_response()
}

async fn post_env_prepare(
    // This env_id is allowed to be in the Path because it would be allocated
    // anyway
    Path(env_id): Path<EnvId>,
    State(state): State<AppState>,
    body: String,
) -> Response {
    let documents = match Environment::deserialize(&body) {
        Ok(documents) => documents,
        Err(e) => return ServerError::from(e).into_response(),
    };

    // TODO: some live state to report to the calling CLI or something would be
    // really nice

    // TODO: clean up existing test

    match Environment::prepare(env_id, documents, state).await {
        Ok(env_id) => (StatusCode::OK, Json(json!({ "id": env_id }))).into_response(),
        Err(e) => ServerError::from(e).into_response(),
    }
}

async fn post_timeline(
    Path((env_id, t_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Response {
    let Some(env_id) = id_or_none(&env_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(t_id) = id_or_none(&t_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match Environment::execute(state, env_id, t_id).await {
        Ok(()) => status_ok(),
        Err(e) => ServerError::from(e).into_response(),
    }
}

async fn delete_timeline(
    Path((env_id, t_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(env_id) = id_or_none(&env_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(t_id) = id_or_none(&t_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match Environment::cleanup_timeline(env_id, t_id, &state).await {
        Ok(_) => status_ok(),
        Err(e) => ServerError::from(e).into_response(),
    }
}

async fn delete_env(
    Path(env_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(env_id) = id_or_none(&env_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match Environment::cleanup(&env_id, &state).await {
        Ok(_) => status_ok(),
        Err(e) => ServerError::from(e).into_response(),
    }
}
