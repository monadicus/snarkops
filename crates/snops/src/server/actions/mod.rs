use std::sync::Arc;

use axum::{
    extract::{Extension, FromRequestParts, Path},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use http::request::Parts;
use serde::{Deserialize, Serialize};
use snops_common::state::{id_or_none, EnvId};

use crate::{env::Environment, schema::NodeTargets, state::AppState};

mod config;
pub mod execute;
pub mod models;
mod power;

#[macro_export]
macro_rules! json_response {
    ( $code:ident , $json:tt $(,)? ) => {
        ::axum::response::IntoResponse::into_response((
            ::http::StatusCode::$code,
            ::axum::Json(::serde_json::json!($json)),
        ))
    };
}

#[derive(Deserialize, Serialize, Clone)]
struct WithTargets<T = ()> {
    nodes: NodeTargets,
    #[serde(flatten)]
    data: T,
}

// /env/:env_id/action/<this router>

#[derive(FromRequestParts)]
struct CommonParams {
    #[from_request(via(Path))]
    env_id: String,
    #[from_request(via(Extension))]
    state: AppState,
}

#[derive(Clone)]
pub struct Env {
    env: Arc<Environment>,
    #[allow(dead_code)]
    env_id: EnvId,
    state: AppState,
}

macro_rules! fake_empty_extractor_state {
    ($name:ty) => {
        #[axum::async_trait]
        impl FromRequestParts<()> for $name {
            type Rejection = ();

            async fn from_request_parts(
                _parts: &mut Parts,
                _state: &(),
            ) -> Result<Self, Self::Rejection> {
                unreachable!()
            }
        }
    };
}

fake_empty_extractor_state!(Env);

#[axum::async_trait]
impl FromRequestParts<AppState> for Env {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let params = CommonParams::from_request_parts(parts, state).await?;

        // get environment
        let env_id = id_or_none(&params.env_id)
            .ok_or_else(|| axum::http::StatusCode::NOT_FOUND.into_response())?;

        let env = state
            .get_env(env_id)
            .ok_or_else(|| axum::http::StatusCode::NOT_FOUND.into_response())?;

        Ok(Self {
            env,
            env_id,
            state: params.state,
        })
    }
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/online", post(power::online))
        .route("/offline", post(power::offline))
        .route("/reboot", post(power::reboot))
        .route("/config", post(config::config))
        .route("/execute", post(execute::execute))
}
