use axum::{response::IntoResponse, Json};
use serde_json::json;
use thiserror::Error;

use crate::{cannon::error::CannonError, env::error::EnvError, schema::error::SchemaError};

#[derive(Debug, Error, strum_macros::AsRefStr)]
// #[serde(tag = "type", content = "data")]
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

impl serde::Serialize for ServerError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use std::fmt::Write;
        let mut s = String::new();
        write!(s, "{}", self).unwrap();
        serializer.serialize_str(&s)
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"errors": [{
								"type": self.as_ref(),
								"error": self,
						}] })),
        )
            .into_response()
    }
}
