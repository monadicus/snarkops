use axum::{response::IntoResponse, Json};
use serde::{ser::SerializeStruct, Serialize, Serializer};
use serde_json::json;
use thiserror::Error;

use crate::{cannon::error::CannonError, env::error::EnvError, schema::error::SchemaError};

#[derive(Debug, Error, strum_macros::AsRefStr)]
pub enum ServerError {
    #[error("failed to initialize the database: {0}")]
    DbInit(#[source] surrealdb::Error),
    #[error("cannon error: {0}")]
    Cannon(#[from] CannonError),
    #[error("cannon error: {0}")]
    Env(#[from] EnvError),
    #[error("cannon error: {0}")]
    Schema(#[from] SchemaError),
    #[error("failed to serve: {0}")]
    Serve(#[source] std::io::Error),
    #[error("failed to bind to tcp: {0}")]
    TcpBind(#[source] std::io::Error),
}

impl ServerError {
    pub fn status_code(&self) -> axum::http::StatusCode {
        match self {
            Self::Cannon(e) => e.status_code(),
            Self::Env(e) => e.status_code(),
            Self::Schema(e) => e.status_code(),
            _ => unreachable!("we should never hit this case since the server isn't running yet"),
        }
    }
}

impl Serialize for ServerError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", self.as_ref())?;

        match self {
            Self::DbInit(e) => state.serialize_field("error", &e.to_string()),
            Self::Cannon(e) => state.serialize_field("error", e),
            Self::Env(e) => state.serialize_field("error", e),
            Self::Schema(e) => state.serialize_field("error", e),
            Self::Serve(e) => state.serialize_field("error", &e.to_string()),
            Self::TcpBind(e) => state.serialize_field("error", &e.to_string()),
        }?;

        state.end()
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        (self.status_code(), Json(json!(self))).into_response()
    }
}
