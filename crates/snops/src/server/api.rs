use std::{collections::HashMap, str::FromStr};

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
    state::{id_or_none, EnvId, NodeKey},
};
use tower::Service;
use tower_http::services::ServeFile;

use super::{error::ServerError, models::AgentStatusResponse, AppState};
use crate::env::{EnvPeer, Environment};
use crate::{cannon::router::redirect_cannon_routes, schema::storage::DEFAULT_AOT_BIN};

#[macro_export]
macro_rules! unwrap_or_not_found {
    ($e:expr) => {
        match $e {
            Some(v) => v,
            None => return ::axum::http::StatusCode::NOT_FOUND.into_response(),
        }
    };
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/agents", get(get_agents))
        .route("/agents/:id", get(get_agent))
        .route("/agents/:id/tps", get(get_agent_tps))
        .route("/env/list", get(get_env_list))
        .route("/env/:env_id/topology", get(get_env_topology))
        .route("/env/:env_id/agents", get(get_env_agents))
        .route(
            "/env/:env_id/agents/:node_ty/:node_key",
            get(get_env_agent_key),
        )
        .route("/env/:env_id/prepare", post(post_env_prepare))
        .route("/env/:env_id/storage", get(get_storage_info))
        .route("/env/:env_id/storage/:ty", get(redirect_storage))
        .nest("/env/:env_id/cannons", redirect_cannon_routes())
        .route("/env/:id", delete(delete_env))
        .route(
            "/env/:env_id/timelines/:timeline_id/steps",
            get(get_timeline),
        )
        .route("/env/:id/timelines/:timeline_id", post(post_timeline))
        .route("/env/:id/timelines/:timeline_id", delete(delete_timeline))
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
    let env_id = unwrap_or_not_found!(id_or_none(&env_id));
    let env = unwrap_or_not_found!(state.envs.get(&env_id));

    Json(env.storage.info()).into_response()
}

async fn redirect_storage(
    Path((env_id, ty)): Path<(String, StorageType)>,
    state: State<AppState>,
    req: Request,
) -> Response {
    let env_id = unwrap_or_not_found!(id_or_none(&env_id));
    let env = unwrap_or_not_found!(state.envs.get(&env_id));

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
    let total = state.pool.len();

    let mut ids = Vec::with_capacity(total);
    let mut num_online = 0;
    let mut num_available = 0;
    let mut num_offline = 0;
    let mut num_vals = 0;
    let mut num_provers = 0;
    let mut num_clients = 0;
    let mut num_computes = 0;
    for agent in state.pool.iter() {
        ids.push(agent.id().to_string());
        match agent.rpc() {
            Some(_) => {
                num_online += 1;
            }
            None => {
                num_offline += 1;
                continue;
            }
        }

        dbg!(agent.id().to_string());

        if dbg!(agent.is_inventory()) {
            num_available += 1;
            continue;
        }

        let mode = dbg!(agent.modes());
        if mode.validator {
            num_vals += 1;
        }
        if mode.prover {
            num_provers += 1;
        }
        if mode.client {
            num_clients += 1;
        }
        if mode.compute {
            num_computes += 1;
        }
    }

    Json(json!({
        "total": total,
        "num_online": num_online,
        "num_offline": num_offline,
        "num_available": num_available,
        "num_vals": num_vals,
        "num_provers": num_provers,
        "num_clients": num_clients,
        "num_computes": num_computes,
        "agents": ids,
    }))
}

fn status_ok() -> Response {
    (StatusCode::OK, Json(json!({"status": "ok"}))).into_response()
}

async fn get_agent(state: State<AppState>, Path(id): Path<String>) -> Response {
    let id = unwrap_or_not_found!(id_or_none(&id));
    let agent = unwrap_or_not_found!(state.pool.get(&id));

    Json(AgentStatusResponse::from(agent.value())).into_response()
}

async fn get_agent_tps(state: State<AppState>, Path(id): Path<String>) -> Response {
    let id = unwrap_or_not_found!(id_or_none(&id));
    let agent = unwrap_or_not_found!(state.pool.get(&id));

    let Some(rpc) = agent.rpc() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    match rpc
        .get_metric(tarpc::context::current(), AgentMetric::Tps)
        .await
    {
        Ok(tps) => tps.to_string().into_response(),
        Err(_e) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn get_env_list(State(state): State<AppState>) -> Response {
    Json(state.envs.iter().map(|e| e.id).collect::<Vec<_>>()).into_response()
}

async fn get_timeline(
    Path((env_id, timeline_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Response {
    let env_id = unwrap_or_not_found!(id_or_none(&env_id));
    let timeline_id = unwrap_or_not_found!(id_or_none(&timeline_id));
    let env = unwrap_or_not_found!(state.envs.get(&env_id));
    let timeline = unwrap_or_not_found!(env.timelines.get(&timeline_id));

    Json(json!({
        "steps": timeline.len(),
    }))
    .into_response()
}

async fn get_timelines(Path(env_id): Path<String>, State(state): State<AppState>) -> Response {
    let env_id = unwrap_or_not_found!(id_or_none(&env_id));
    let env = unwrap_or_not_found!(state.envs.get(&env_id));

    Json(&env.timelines.iter().map(|t| *t.key()).collect::<Vec<_>>()).into_response()
}

async fn get_env_topology(Path(env_id): Path<String>, State(state): State<AppState>) -> Response {
    let env_id = unwrap_or_not_found!(id_or_none(&env_id));
    let env = unwrap_or_not_found!(state.envs.get(&env_id));

    Json(&env.node_states).into_response()
}

/// Get a map of node keys to agent ids
async fn get_env_agents(Path(env_id): Path<String>, State(state): State<AppState>) -> Response {
    let env_id = unwrap_or_not_found!(id_or_none(&env_id));
    let env = unwrap_or_not_found!(state.envs.get(&env_id));

    Json(
        env.node_peers
            .iter()
            .filter_map(|(k, v)| match v {
                EnvPeer::Internal(id) => Some((k, *id)),
                _ => None,
            })
            .collect::<HashMap<_, _>>(),
    )
    .into_response()
}

/// Given a node key, get the agent id and connection status
async fn get_env_agent_key(
    Path((env_id, node_type, node_key)): Path<(String, String, String)>,
    State(state): State<AppState>,
) -> Response {
    let node_key = unwrap_or_not_found!(NodeKey::from_str(&format!("{node_type}/{node_key}")).ok());
    let env_id = unwrap_or_not_found!(id_or_none(&env_id));
    let env = unwrap_or_not_found!(state.envs.get(&env_id));
    let agent_id = unwrap_or_not_found!(env.get_agent_by_key(&node_key));
    let agent = unwrap_or_not_found!(state.pool.get(&agent_id));

    Json(AgentStatusResponse::from(agent.value())).into_response()
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
    Path((env_id, timeline_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Response {
    let env_id = unwrap_or_not_found!(id_or_none(&env_id));
    let timeline_id = unwrap_or_not_found!(id_or_none(&timeline_id));

    match Environment::execute(state, env_id, timeline_id).await {
        Ok(()) => status_ok(),
        Err(e) => ServerError::from(e).into_response(),
    }
}

async fn delete_timeline(
    Path((env_id, timeline_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Response {
    let env_id = unwrap_or_not_found!(id_or_none(&env_id));
    let timeline_id = unwrap_or_not_found!(id_or_none(&timeline_id));

    match Environment::cleanup_timeline(env_id, timeline_id, &state).await {
        Ok(_) => status_ok(),
        Err(e) => ServerError::from(e).into_response(),
    }
}

async fn delete_env(Path(env_id): Path<String>, State(state): State<AppState>) -> Response {
    let env_id = unwrap_or_not_found!(id_or_none(&env_id));

    match Environment::cleanup(env_id, &state).await {
        Ok(_) => status_ok(),
        Err(e) => ServerError::from(e).into_response(),
    }
}
