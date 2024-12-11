use std::{collections::HashMap, str::FromStr};

use axum::{
    extract::{self, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use indexmap::IndexSet;
use serde::Deserialize;
use serde_json::json;
use snops_common::{
    key_source::KeySource,
    lasso::Spur,
    node_targets::NodeTargets,
    rpc::control::agent::AgentMetric,
    schema::cannon::source::QueryTarget,
    state::{id_or_none, AgentModeOptions, AgentState, CannonId, EnvId, KeyState, NodeKey},
};
use tarpc::context;

use super::{actions, error::ServerError, event_ws, models::AgentStatusResponse};
use crate::{cannon::router::redirect_cannon_routes, make_env_filter, state::AppState};
use crate::{
    env::{EnvPeer, Environment},
    state::AgentFlags,
};

#[macro_export]
macro_rules! unwrap_or_not_found {
    ($s:expr, $e:expr) => {
        match $e {
            Some(v) => v,
            None => return ServerError::NotFound($s.to_owned()).into_response(),
        }
    };
}

#[macro_export]
macro_rules! unwrap_or_bad_request {
    ($s:expr, $e:expr) => {
        match $e {
            Some(v) => v,
            None => return ServerError::BadRequest($s.to_owned()).into_response(),
        }
    };
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/events", get(event_ws::event_ws_handler))
        .route("/log/:level", post(set_log_level))
        .route("/agents", get(get_agents))
        .route("/agents/:id", get(get_agent))
        .route("/agents/:id/status", get(get_agent_status))
        .route("/agents/:id/kill", post(kill_agent))
        .route("/agents/:id/tps", get(get_agent_tps))
        .route("/agents/:id/log/:level", post(set_agent_log_level))
        .route("/agents/:id/aot/log/:verbosity", post(set_aot_log_level))
        .route("/agents/find", post(find_agents))
        .route("/env/list", get(get_env_list))
        .route("/env/:env_id/topology", get(get_env_topology))
        .route(
            "/env/:env_id/topology/resolved",
            get(get_env_topology_resolved),
        )
        .route("/env/:env_id/agents", get(get_env_agents))
        .route(
            "/env/:env_id/agents/:node_ty/:node_key",
            get(get_env_agent_key),
        )
        // .route(
        //     "/env/:env_id/agents/:node_ty/:node_key/action/status",
        //     get(get_env_agent_key),
        // )
        // .route("/env/:env_id/metric/:prom_ql", get())
        .route("/env/:env_id/apply", post(post_env_apply))
        .route("/env/:env_id/info", get(get_env_info))
        .route("/env/:env_id/height", get(get_latest_height))
        .route("/env/:env_id/block_info", get(get_env_block_info))
        .route("/env/:env_id/balance/:key", get(get_env_balance))
        .route("/env/:env_id/block/:height_or_hash", get(get_block))
        .route(
            "/env/:env_id/transaction_block/:tx_id",
            get(get_tx_blockhash),
        )
        .route("/env/:env_id/transaction/:tx_id", get(get_tx))
        .route("/env/:env_id/program/:program", get(get_program))
        .route(
            "/env/:env_id/program/:program/mapping/:mapping",
            get(get_mapping_value),
        )
        .route("/env/:env_id/program/:program/mappings", get(get_mappings))
        .nest("/env/:env_id/cannons", redirect_cannon_routes())
        .route("/env/:id", delete(delete_env))
        .nest("/env/:env_id/action", actions::routes())
}

async fn set_agent_log_level(
    state: State<AppState>,
    Path((id, level)): Path<(String, String)>,
) -> Response {
    let id = unwrap_or_not_found!("unknown agent id", id_or_none(&id));
    let agent = unwrap_or_not_found!("agent not found", state.pool.get(&id));

    tracing::debug!("attempting to set agent log level to {level} for agent {id}");
    let Some(rpc) = agent.client_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    let Err(e) = rpc.0.set_log_level(tarpc::context::current(), level).await else {
        return status_ok();
    };

    ServerError::from(e).into_response()
}

async fn set_aot_log_level(
    state: State<AppState>,
    Path((id, verbosity)): Path<(String, u8)>,
) -> Response {
    let id = unwrap_or_not_found!("unknown agent id", id_or_none(&id));
    let agent = unwrap_or_not_found!("agent not found", state.pool.get(&id));

    tracing::debug!("attempting to set aot log verbosity to {verbosity}  for agent {id}");
    let Some(rpc) = agent.rpc() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    // let mut ctx = tarpc::context::current();
    // ctx.deadline += std::time::Duration::from_secs(300);
    let Err(e) = rpc
        .set_aot_log_level(tarpc::context::current(), verbosity)
        .await
    else {
        return status_ok();
    };

    ServerError::from(e).into_response()
}

async fn set_log_level(Path(level): Path<String>, state: State<AppState>) -> Response {
    tracing::debug!("attempting to set log level to {level}");
    let Ok(level) = level.parse() else {
        return ServerError::InvalidLogLevel(level).into_response();
    };
    tracing::info!("Setting log level to {level}");
    let Ok(_) = state
        .log_level_handler
        .modify(|filter| *filter = make_env_filter(level))
    else {
        return ServerError::FailedToChangeLogLevel.into_response();
    };

    status_ok()
}

async fn get_env_info(Path(env_id): Path<String>, state: State<AppState>) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let env = unwrap_or_not_found!("environment not found", state.get_env(env_id));

    Json(env.info(&state)).into_response()
}

async fn get_latest_height(Path(env_id): Path<String>, state: State<AppState>) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let env = unwrap_or_not_found!("environment not found", state.get_env(env_id));

    let cannon = unwrap_or_not_found!(
        "default cannon not found",
        env.get_cannon(CannonId::default())
    );

    match &cannon.source.query {
        QueryTarget::Local(_qs) => StatusCode::NOT_IMPLEMENTED.into_response(),
        QueryTarget::Node(target) => {
            match state
                .snarkos_get::<Option<u32>>(env_id, "/block/height/latest".to_string(), target)
                .await
            {
                Ok(res) => Json(res).into_response(),
                Err(e) => ServerError::from(e).into_response(),
            }
        }
    }
}

async fn get_env_block_info(Path(env_id): Path<String>, state: State<AppState>) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let block_info =
        unwrap_or_not_found!("environment not found", state.get_env_block_info(env_id));

    Json(block_info).into_response()
}

async fn get_env_balance(
    Path((env_id, keysource)): Path<(String, KeySource)>,
    state: State<AppState>,
) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let env = unwrap_or_not_found!("environment not found", state.get_env(env_id));

    let KeyState::Literal(key) = env.storage.sample_keysource_addr(&keysource) else {
        return ServerError::NotFound(format!("keysource pubkey {keysource}")).into_response();
    };

    let Some(cannon) = env.get_cannon(CannonId::default()) else {
        return ServerError::NotFound("default cannon not found".to_owned()).into_response();
    };

    match &cannon.source.query {
        QueryTarget::Local(_qs) => StatusCode::NOT_IMPLEMENTED.into_response(),
        QueryTarget::Node(target) => {
            match state
                .snarkos_get::<Option<String>>(
                    env_id,
                    format!("/program/credits.aleo/mapping/account/{key}"),
                    target,
                )
                .await
            {
                Ok(None) => "0".to_string().into_response(),
                Ok(Some(value)) => if let Some(balance) = value
                    .strip_suffix("u64")
                    .and_then(|s| u64::from_str(s).ok())
                {
                    balance.to_string().into_response()
                } else {
                    (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({ "error": format!("unexpected value '{value}'") })),
                    )
                        .into_response()
                }
                .into_response(),
                Err(e) => ServerError::from(e).into_response(),
            }
        }
    }
}

async fn get_block(
    Path((env_id, height_or_hash)): Path<(String, String)>,
    state: State<AppState>,
) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let env = unwrap_or_not_found!("environment not found", state.get_env(env_id));
    let cannon = unwrap_or_not_found!(
        "default cannon not found",
        env.get_cannon(CannonId::default())
    );

    match &cannon.source.query {
        QueryTarget::Local(_qs) => StatusCode::NOT_IMPLEMENTED.into_response(),
        QueryTarget::Node(target) => {
            match state
                .snarkos_get::<Option<serde_json::Value>>(
                    env_id,
                    format!("/block/{height_or_hash}"),
                    target,
                )
                .await
            {
                Ok(res) => Json(res).into_response(),
                Err(e) => ServerError::from(e).into_response(),
            }
        }
    }
}

async fn get_tx_blockhash(
    Path((env_id, transaction)): Path<(String, String)>,
    state: State<AppState>,
) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let env = unwrap_or_not_found!("environment not found", state.get_env(env_id));
    let cannon = unwrap_or_not_found!(
        "default cannon not found",
        env.get_cannon(CannonId::default())
    );

    match &cannon.source.query {
        QueryTarget::Local(_qs) => StatusCode::NOT_IMPLEMENTED.into_response(),
        QueryTarget::Node(target) => {
            match state
                .snarkos_get::<Option<String>>(
                    env_id,
                    format!("/find/blockHash/{transaction}"),
                    target,
                )
                .await
            {
                Ok(res) => Json(res).into_response(),
                Err(e) => ServerError::from(e).into_response(),
            }
        }
    }
}

async fn get_tx(
    Path((env_id, transaction)): Path<(String, String)>,
    state: State<AppState>,
) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let env = unwrap_or_not_found!("environment not found", state.get_env(env_id));
    let cannon = unwrap_or_not_found!(
        "default cannon not found",
        env.get_cannon(CannonId::default())
    );

    match &cannon.source.query {
        QueryTarget::Local(_qs) => StatusCode::NOT_IMPLEMENTED.into_response(),
        QueryTarget::Node(target) => {
            match state
                .snarkos_get::<Option<serde_json::Value>>(
                    env_id,
                    format!("/transaction/{transaction}"),
                    target,
                )
                .await
            {
                Ok(res) => Json(res).into_response(),
                Err(e) => ServerError::from(e).into_response(),
            }
        }
    }
}

async fn get_agents(state: State<AppState>) -> impl IntoResponse {
    let agents = state
        .pool
        .iter()
        .map(|agent| AgentStatusResponse::from(agent.value()))
        .collect::<Vec<_>>();

    Json(agents).into_response()
}

fn status_ok() -> Response {
    (StatusCode::OK, Json(json!({"status": "ok"}))).into_response()
}

async fn get_agent(state: State<AppState>, Path(id): Path<String>) -> Response {
    let id = unwrap_or_not_found!("unknown agent id", id_or_none(&id));
    let agent = unwrap_or_not_found!("agent not found", state.pool.get(&id));

    Json(AgentStatusResponse::from(agent.value())).into_response()
}

async fn get_agent_status(state: State<AppState>, Path(id): Path<String>) -> Response {
    let id = unwrap_or_not_found!("unknown agent id", id_or_none(&id));
    let agent = unwrap_or_not_found!("agent not found", state.pool.get(&id));

    let Some(rpc) = agent.rpc() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    match rpc.get_status(tarpc::context::current()).await {
        Ok(status) => Json(status).into_response(),
        Err(_e) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn kill_agent(state: State<AppState>, Path(id): Path<String>) -> Response {
    let id = unwrap_or_not_found!("unknown agent id", id_or_none(&id));
    let client = unwrap_or_not_found!(
        "agent not found",
        state.pool.get(&id).and_then(|a| a.client_owned())
    );

    if let Err(e) = client.0.kill(context::current()).await {
        tracing::error!("failed to kill agent {id}: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "rpc error"})),
        )
            .into_response();
    }

    Json("ok").into_response()
}

async fn get_agent_tps(state: State<AppState>, Path(id): Path<String>) -> Response {
    let id = unwrap_or_not_found!("unknown agent id", id_or_none(&id));
    let agent = unwrap_or_not_found!("agent not found", state.pool.get(&id));

    let Some(rpc) = agent.client_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    match rpc
        .0
        .get_metric(tarpc::context::current(), AgentMetric::Tps)
        .await
    {
        Ok(tps) => tps.to_string().into_response(),
        Err(_e) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn get_program(
    Path((env_id, program)): Path<(String, String)>,
    state: State<AppState>,
) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    match state
        .snarkos_get::<String>(env_id, format!("/program/{program}"), &NodeTargets::ALL)
        .await
    {
        Ok(program) => program.into_response(),
        Err(e) => ServerError::from(e).into_response(),
    }
}

#[derive(Deserialize)]
struct MappingValueQuery {
    key: Option<String>,
    keysource: Option<KeySource>,
}

async fn get_mapping_value(
    Path((env_id, program, mapping)): Path<(String, String, String)>,
    Query(query): Query<MappingValueQuery>,
    state: State<AppState>,
) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let env = unwrap_or_not_found!("environment not found", state.get_env(env_id));
    let cannon = unwrap_or_not_found!(
        "default cannon not found",
        env.get_cannon(CannonId::default())
    );

    let url = match (query.key, query.keysource) {
        (Some(key), None) => {
            format!("/program/{program}/mapping/{mapping}/{key}",)
        }
        (None, Some(keysource)) => {
            let KeyState::Literal(key) = env.storage.sample_keysource_addr(&keysource) else {
                return ServerError::NotFound(format!("keysource pubkey {keysource}"))
                    .into_response();
            };
            format!("/program/{program}/mapping/{mapping}/{key}",)
        }
        _ => {
            return ServerError::BadRequest("either key or key_source must be provided".to_owned())
                .into_response()
        }
    };

    match &cannon.source.query {
        QueryTarget::Local(_qs) => StatusCode::NOT_IMPLEMENTED.into_response(),
        QueryTarget::Node(target) => {
            match state
                .snarkos_get::<Option<String>>(env_id, url, target)
                .await
            {
                Ok(value) => Json(json!({"value": value})).into_response(),
                Err(e) => ServerError::from(e).into_response(),
            }
        }
    }
}

async fn get_mappings(
    Path((env_id, program)): Path<(String, String)>,
    state: State<AppState>,
) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let env = unwrap_or_not_found!("environment not found", state.get_env(env_id));
    let cannon = unwrap_or_not_found!(
        "default cannon not found",
        env.get_cannon(CannonId::default())
    );

    match &cannon.source.query {
        QueryTarget::Local(_qs) => StatusCode::NOT_IMPLEMENTED.into_response(),
        QueryTarget::Node(target) => {
            match state
                .snarkos_get::<Vec<String>>(env_id, format!("/program/{program}/mappings"), target)
                .await
            {
                Ok(mappings) => Json(mappings).into_response(),
                Err(e) => ServerError::from(e).into_response(),
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct FindAgents {
    mode: AgentModeOptions,
    env: Option<EnvId>,
    #[serde(default, deserialize_with = "snops_common::schema::nodes::deser_label")]
    labels: IndexSet<Spur>,
    all: bool,
    include_offline: bool,
    local_pk: bool,
}

async fn find_agents(
    State(state): State<AppState>,
    extract::Json(payload): extract::Json<FindAgents>,
) -> Response {
    let labels_vec = payload.labels.iter().copied().collect::<Vec<_>>();
    let mask = AgentFlags {
        mode: payload.mode,
        labels: payload.labels,
        local_pk: payload.local_pk,
    }
    .mask(&labels_vec);
    let agents = state
        .pool
        .iter()
        .filter(|agent| {
            // This checks the mode, labels, and local_pk.
            let mask_matches = mask.is_subset(&agent.mask(&labels_vec));

            let env_matches = if payload.all {
                // if we ask for all env we just say true
                true
            } else if let Some(env) = payload.env {
                // otherwise if the env is specified we check it matches
                agent.env().map_or(false, |a_env| env == a_env)
            } else {
                // if no env is specified
                agent.state() == &AgentState::Inventory
            };

            // if all is specified we don't care about whether an agent's connection
            // if include_offline is true we also get both online and offline agents.
            let connected_match = payload.all || payload.include_offline || agent.is_connected();

            mask_matches && env_matches && connected_match
        })
        .map(|a| AgentStatusResponse::from(a.value()))
        .collect::<Vec<_>>();

    Json(agents).into_response()
}

async fn get_env_list(State(state): State<AppState>) -> Response {
    Json(state.envs.iter().map(|e| e.id).collect::<Vec<_>>()).into_response()
}

async fn get_env_topology(Path(env_id): Path<String>, State(state): State<AppState>) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let env = unwrap_or_not_found!("environment not found", state.get_env(env_id));

    let mut internal = HashMap::new();
    let mut external = HashMap::new();

    for (nk, peer) in env.node_peers.iter() {
        let Some(node_state) = env.node_states.get(nk) else {
            continue;
        };
        match peer {
            EnvPeer::Internal(id) => {
                internal.insert(*id, node_state);
            }
            EnvPeer::External(ip) => {
                external.insert(
                    nk.to_string(),
                    json!({"ip": ip.to_string(), "ports": node_state}),
                );
            }
        }
    }

    Json(json!({"internal": internal, "external": external })).into_response()
}

async fn get_env_topology_resolved(
    Path(env_id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let env = unwrap_or_not_found!("environment not found", state.get_env(env_id));

    let mut resolved = HashMap::new();

    for (_, peer) in env.node_peers.iter() {
        if let EnvPeer::Internal(id) = peer {
            let Some(agent) = state.pool.get(id) else {
                continue;
            };
            match agent.state().clone() {
                AgentState::Inventory => continue,
                AgentState::Node(_, state) => {
                    resolved.insert(*id, state);
                }
            }
        }
    }

    Json(resolved).into_response()
}

/// Get a map of node keys to agent ids
async fn get_env_agents(Path(env_id): Path<String>, State(state): State<AppState>) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let env = unwrap_or_not_found!("environment not found", state.get_env(env_id));

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
    let node_key = unwrap_or_bad_request!(
        "invalid node key",
        NodeKey::from_str(&format!("{node_type}/{node_key}")).ok()
    );
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));
    let env = unwrap_or_not_found!("environment not found", state.get_env(env_id));
    let agent_id =
        unwrap_or_not_found!("node found in environment", env.get_agent_by_key(&node_key));
    let agent = unwrap_or_not_found!("agent not found", state.pool.get(&agent_id));

    Json(AgentStatusResponse::from(agent.value())).into_response()
}

async fn post_env_apply(
    // This env_id is allowed to be in the Path because it would be allocated
    // anyway
    Path(env_id): Path<EnvId>,
    State(state): State<AppState>,
    body: String,
) -> Response {
    let documents = match snops_common::schema::deserialize_docs(&body) {
        Ok(documents) => documents,
        Err(e) => return ServerError::from(e).into_response(),
    };

    match Environment::apply(env_id, documents, state).await {
        Ok(node_map) => Json(json!(node_map)).into_response(),
        Err(e) => ServerError::from(e).into_response(),
    }
}

async fn delete_env(Path(env_id): Path<String>, State(state): State<AppState>) -> Response {
    let env_id = unwrap_or_not_found!("unknown environment id", id_or_none(&env_id));

    match Environment::cleanup(env_id, &state).await {
        Ok(_) => status_ok(),
        Err(e) => ServerError::from(e).into_response(),
    }
}
