use std::{str::FromStr, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use snops_common::{
    key_source::KeySource,
    state::{Authorization, KeyState, NetworkId, id_or_none},
};

use super::source::QueryTarget;
use crate::{
    server::{actions::execute::execute_status, error::ServerError},
    state::AppState,
};

pub(crate) fn redirect_cannon_routes() -> Router<AppState> {
    Router::new()
        .route("/:cannon/:network/latest/stateRoot", get(state_root))
        .route("/:cannon/:network/stateRoot/latest", get(state_root))
        .route("/:cannon/:network/transaction/broadcast", post(transaction))
        .route(
            "/:cannon/:network/find/blockHash/:tx",
            get(get_tx_blockhash),
        )
        .route(
            "/:cannon/:network/block/height/latest",
            get(get_latest_height),
        )
        .route("/:cannon/:network/block/:height_or_hash", get(get_block))
        .route("/:cannon/:network/program/:program", get(get_program_json))
        .route(
            "/:cannon/:network/program/:program/mappings",
            get(get_mappings_json),
        )
        .route(
            "/:cannon/:network/program/:program/mapping/:mapping/:value",
            get(get_mapping_json),
        )
        .route("/:cannon/auth", post(authorization))
}

async fn state_root(
    Path((env_id, cannon_id, network)): Path<(String, String, NetworkId)>,
    state: State<AppState>,
) -> Response {
    let (Some(env_id), Some(cannon_id)) = (id_or_none(&env_id), id_or_none(&cannon_id)) else {
        return ServerError::NotFound("unknown cannon or environment".to_owned()).into_response();
    };

    let Some(env) = state.get_env(env_id) else {
        return ServerError::NotFound("environment not found".to_owned()).into_response();
    };

    if env.network != network {
        return ServerError::NotFound("network mismatch".to_owned()).into_response();
    }

    let Some(cannon) = env.get_cannon(cannon_id) else {
        return ServerError::NotFound("cannon not found".to_owned()).into_response();
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
                    StatusCode::REQUEST_TIMEOUT,
                    Json(json!({ "error": "non-responsive query node", "inner": format!("{e}") })),
                )
                    .into_response();
            }

            _ => attempts += 1,
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn get_latest_height(
    Path((env_id, cannon_id, network)): Path<(String, String, NetworkId)>,
    state: State<AppState>,
) -> Response {
    let (Some(env_id), Some(cannon_id)) = (id_or_none(&env_id), id_or_none(&cannon_id)) else {
        return ServerError::NotFound("unknown cannon or environment".to_owned()).into_response();
    };

    let Some(env) = state.get_env(env_id) else {
        return ServerError::NotFound("environment not found".to_owned()).into_response();
    };

    if env.network != network {
        return ServerError::NotFound("network mismatch".to_owned()).into_response();
    }

    let Some(cannon) = env.get_cannon(cannon_id) else {
        return ServerError::NotFound("cannon not found".to_owned()).into_response();
    };

    // TODO: lock this with a mutex or something so that multiple route callers
    // can't bombard the cannon with proxy_state_root call attempts
    let mut attempts = 0;
    loop {
        attempts += 1;
        match cannon.proxy_latest_height().await {
            Ok(height) => break Json(height).into_response(),

            Err(e) if attempts > 5 => {
                break (
                    StatusCode::REQUEST_TIMEOUT,
                    Json(json!({ "error": "non-responsive query node", "inner": format!("{e}") })),
                )
                    .into_response();
            }

            _ => attempts += 1,
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn get_program_json(
    Path((env_id, cannon_id, network, program)): Path<(String, String, NetworkId, String)>,
    state: State<AppState>,
) -> Response {
    let (Some(env_id), Some(cannon_id)) = (id_or_none(&env_id), id_or_none(&cannon_id)) else {
        return ServerError::NotFound("unknown cannon or environment".to_owned()).into_response();
    };

    let Some(env) = state.get_env(env_id) else {
        return ServerError::NotFound("environment not found".to_owned()).into_response();
    };

    if env.network != network {
        return ServerError::NotFound("network mismatch".to_owned()).into_response();
    }

    let Some(cannon) = env.get_cannon(cannon_id) else {
        return ServerError::NotFound("cannon not found".to_owned()).into_response();
    };

    match &cannon.source.query {
        QueryTarget::Local(_qs) => StatusCode::NOT_IMPLEMENTED.into_response(),
        QueryTarget::Node(target) => {
            match state
                .snarkos_get::<String>(env_id, format!("/program/{program}"), target)
                .await
            {
                Ok(program) => Json(program).into_response(),
                Err(e) => ServerError::from(e).into_response(),
            }
        }
    }
}

async fn get_mappings_json(
    Path((env_id, cannon_id, network, program)): Path<(String, String, NetworkId, String)>,
    state: State<AppState>,
) -> Response {
    let (Some(env_id), Some(cannon_id)) = (id_or_none(&env_id), id_or_none(&cannon_id)) else {
        return ServerError::NotFound("unknown cannon or environment".to_owned()).into_response();
    };

    let Some(env) = state.get_env(env_id) else {
        return ServerError::NotFound("environment not found".to_owned()).into_response();
    };

    if env.network != network {
        return ServerError::NotFound("network mismatch".to_owned()).into_response();
    }

    let Some(cannon) = env.get_cannon(cannon_id) else {
        return ServerError::NotFound("cannon not found".to_owned()).into_response();
    };

    match &cannon.source.query {
        QueryTarget::Local(_qs) => StatusCode::NOT_IMPLEMENTED.into_response(),
        QueryTarget::Node(target) => {
            match state
                .snarkos_get::<Vec<String>>(env_id, format!("/program/{program}/mappings"), target)
                .await
            {
                Ok(res) => Json(res).into_response(),
                Err(e) => ServerError::from(e).into_response(),
            }
        }
    }
}

async fn get_tx_blockhash(
    Path((env_id, cannon_id, network, transaction)): Path<(String, String, NetworkId, String)>,
    state: State<AppState>,
) -> Response {
    let (Some(env_id), Some(cannon_id)) = (id_or_none(&env_id), id_or_none(&cannon_id)) else {
        return ServerError::NotFound("unknown cannon or environment".to_owned()).into_response();
    };

    let Some(env) = state.get_env(env_id) else {
        return ServerError::NotFound("environment not found".to_owned()).into_response();
    };

    if env.network != network {
        return ServerError::NotFound("network mismatch".to_owned()).into_response();
    }

    let Some(cannon) = env.get_cannon(cannon_id) else {
        return ServerError::NotFound("cannon not found".to_owned()).into_response();
    };

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

async fn get_block(
    Path((env_id, cannon_id, network, height_or_hash)): Path<(String, String, NetworkId, String)>,
    state: State<AppState>,
) -> Response {
    let (Some(env_id), Some(cannon_id)) = (id_or_none(&env_id), id_or_none(&cannon_id)) else {
        return ServerError::NotFound("unknown cannon or environment".to_owned()).into_response();
    };

    let Some(env) = state.get_env(env_id) else {
        return ServerError::NotFound("environment not found".to_owned()).into_response();
    };

    if env.network != network {
        return ServerError::NotFound("network mismatch".to_owned()).into_response();
    }

    let Some(cannon) = env.get_cannon(cannon_id) else {
        return ServerError::NotFound("cannon not found".to_owned()).into_response();
    };

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

#[derive(Debug, Deserialize)]
struct MappingQuery {
    keysource: Option<bool>,
}

async fn get_mapping_json(
    Path((env_id, cannon_id, network, program, mapping, mut mapping_key)): Path<(
        String,
        String,
        NetworkId,
        String,
        String,
        String,
    )>,
    query: Query<MappingQuery>,
    state: State<AppState>,
) -> Response {
    let (Some(env_id), Some(cannon_id)) = (id_or_none(&env_id), id_or_none(&cannon_id)) else {
        return ServerError::NotFound("unknown cannon or environment".to_owned()).into_response();
    };

    let Some(env) = state.get_env(env_id) else {
        return ServerError::NotFound("environment not found".to_owned()).into_response();
    };

    if query.keysource.unwrap_or_default() {
        let keysource = match KeySource::from_str(&mapping_key) {
            Ok(ks) => ks,
            Err(e) => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({ "error": format!("invalid keysource: {e}") })),
                )
                    .into_response();
            }
        };

        let KeyState::Literal(found) = env.storage.sample_keysource_addr(&keysource) else {
            return ServerError::NotFound(format!("keysource pubkey {mapping_key}"))
                .into_response();
        };
        mapping_key = found;
    }

    if env.network != network {
        return ServerError::NotFound("network mismatch".to_owned()).into_response();
    }

    let Some(cannon) = env.get_cannon(cannon_id) else {
        return ServerError::NotFound("cannon not found".to_owned()).into_response();
    };

    match &cannon.source.query {
        QueryTarget::Local(_qs) => StatusCode::NOT_IMPLEMENTED.into_response(),
        QueryTarget::Node(target) => {
            match state
                .snarkos_get::<Option<String>>(
                    env_id,
                    format!("/program/{program}/mapping/{mapping}/{mapping_key}"),
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

async fn transaction(
    Path((env_id, cannon_id, network)): Path<(String, String, NetworkId)>,
    state: State<AppState>,
    mut body: Json<serde_json::Value>,
) -> Response {
    let (Some(env_id), Some(cannon_id)) = (id_or_none(&env_id), id_or_none(&cannon_id)) else {
        return ServerError::NotFound("unknown cannon or environment".to_owned()).into_response();
    };

    let Some(env) = state.get_env(env_id) else {
        return ServerError::NotFound("environment not found".to_owned()).into_response();
    };

    if env.network != network {
        return ServerError::NotFound("network mismatch".to_owned()).into_response();
    }

    let Some(cannon) = env.get_cannon(cannon_id) else {
        return ServerError::NotFound("cannon not found".to_owned()).into_response();
    };

    let Some(tx_id) = body.get("id").and_then(|id| id.as_str().map(str::to_owned)) else {
        return ServerError::BadRequest("body missing transaction ID".to_owned()).into_response();
    };

    match cannon.proxy_broadcast(Arc::new(tx_id), body.take()) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => ServerError::from(e).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct AuthQuery {
    /// When present, the response will contain only the transaction ID
    #[serde(rename = "async")]
    async_mode: Option<bool>,
}

impl AuthQuery {
    pub fn is_async(&self) -> bool {
        self.async_mode.unwrap_or_default()
    }
}

async fn authorization(
    Path((env_id, cannon_id)): Path<(String, String)>,
    state: State<AppState>,
    Query(query): Query<AuthQuery>,
    Json(body): Json<Authorization>,
) -> Response {
    let (Some(env_id), Some(cannon_id)) = (id_or_none(&env_id), id_or_none(&cannon_id)) else {
        return ServerError::NotFound("unknown cannon or environment".to_owned()).into_response();
    };

    let Some(env) = state.get_env(env_id) else {
        return ServerError::NotFound("environment not found".to_owned()).into_response();
    };

    let Some(cannon) = env.get_cannon(cannon_id) else {
        return ServerError::NotFound("cannon not found".to_owned()).into_response();
    };

    if query.is_async() {
        return match cannon.proxy_auth(body).await {
            Ok(tx_id) => (StatusCode::ACCEPTED, Json(tx_id)).into_response(),
            Err(e) => ServerError::from(e).into_response(),
        };
    }

    match cannon.proxy_auth(body).await {
        Ok(tx_id) => {
            use snops_common::events::EventFilter::*;
            let subscriber = state
                .events
                .subscribe_on(TransactionIs(tx_id.clone()) & EnvIs(env_id) & CannonIs(cannon_id));
            execute_status(tx_id, subscriber).await.into_response()
        }
        Err(e) => ServerError::from(e).into_response(),
    }
}
