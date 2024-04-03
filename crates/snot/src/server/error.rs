use axum::{response::IntoResponse, Json};
use http::StatusCode;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use serde_json::json;
use snot_common::{impl_into_status_code, state::AgentId};
use thiserror::Error;

use crate::{
    cannon::error::CannonError, env::error::EnvError, error::DeserializeError,
    schema::error::SchemaError,
};

#[derive(Debug, Error, strum_macros::AsRefStr)]
pub enum ServerError {
    #[error("agent `{0}` not found")]
    AgentNotFound(AgentId),
    #[error("cannon error: {0}")]
    Cannon(#[from] CannonError),
    #[error("deserialize error: {0}")]
    Deserialize(#[from] DeserializeError),
    #[error("env error: {0}")]
    Env(#[from] EnvError),
    #[error("schema error: {0}")]
    Schema(#[from] SchemaError),
}

impl_into_status_code!(ServerError, |value| match value {
    ServerError::AgentNotFound(_) => axum::http::StatusCode::NOT_FOUND,
    ServerError::Cannon(e) => e.into(),
    ServerError::Deserialize(e) => e.into(),
    ServerError::Env(e) => e.into(),
    ServerError::Schema(e) => e.into(),
});

impl Serialize for ServerError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", self.as_ref())?;

        match self {
            Self::Cannon(e) => state.serialize_field("error", e),
            Self::Deserialize(e) => state.serialize_field("error", &e.to_string()),
            Self::Env(e) => state.serialize_field("error", e),
            Self::Schema(e) => state.serialize_field("error", e),
            _ => state.serialize_field("error", &self.to_string()),
        }?;

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
    #[error("failed to initialize the database: {0}")]
    DbInit(#[from] surrealdb::Error),
    #[error("failed to serve: {0}")]
    Serve(#[source] std::io::Error),
    #[error("failed to bind to tcp: {0}")]
    TcpBind(#[source] std::io::Error),
}
