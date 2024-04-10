use std::path::PathBuf;

use axum::{response::IntoResponse, Json};
use http::StatusCode;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use serde_json::json;
use snops_common::{impl_into_status_code, impl_into_type_str, state::AgentId};
use thiserror::Error;

use crate::{
    cannon::error::CannonError, env::error::EnvError, error::DeserializeError,
    schema::error::SchemaError,
};

#[derive(Debug, Error, strum_macros::AsRefStr)]
pub enum ServerError {
    #[error("agent `{0}` not found")]
    AgentNotFound(AgentId),
    #[error(transparent)]
    Cannon(#[from] CannonError),
    #[error(transparent)]
    Deserialize(#[from] DeserializeError),
    #[error(transparent)]
    Env(#[from] EnvError),
    #[error(transparent)]
    Schema(#[from] SchemaError),
}

impl_into_status_code!(ServerError, |value| match value {
    AgentNotFound(_) => axum::http::StatusCode::NOT_FOUND,
    Cannon(e) => e.into(),
    Deserialize(e) => e.into(),
    Env(e) => e.into(),
    Schema(e) => e.into(),
});

impl_into_type_str!(ServerError, |value| match value {
    Cannon(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Env(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Schema(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    _ => value.as_ref().to_string(),
});

impl Serialize for ServerError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", &String::from(self))?;
        state.serialize_field("error", &self.to_string())?;

        state.end()
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        let json = json!(self);
        let mut res = (StatusCode::from(&self), Json(&json)).into_response();

        res.extensions_mut().insert(json);
        res
    }
}

#[derive(Debug, Error, strum_macros::AsRefStr)]
pub enum StartError {
    #[error("failed to create db file at {0:?}: {1}")]
    DbCreate(PathBuf, #[source] sqlx::Error),
    #[error("failed to connect to db: {0}")]
    DbConnect(#[source] sqlx::Error),
    #[error("failed to serve: {0}")]
    Serve(#[source] std::io::Error),
    #[error("failed to bind to tcp: {0}")]
    TcpBind(#[source] std::io::Error),
}
