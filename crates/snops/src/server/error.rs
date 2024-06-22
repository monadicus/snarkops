use axum::{response::IntoResponse, Json};
use http::StatusCode;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use serde_json::json;
use snops_common::{
    aot_cmds::AotCmdError, db::error::DatabaseError, impl_into_status_code, impl_into_type_str,
};
use thiserror::Error;

use crate::{
    cannon::error::CannonError,
    db::error::DatabaseError,
    env::error::{EnvError, EnvRequestError, ExecutionError},
    error::DeserializeError,
    schema::error::SchemaError,
    state::error::BatchReconcileError,
};

#[derive(Debug, Error, strum_macros::AsRefStr)]
pub enum ServerError {
    #[error(transparent)]
    BatchReconcile(#[from] BatchReconcileError),
    #[error("Content resource `{0}` not found")]
    ContentNotFound(String),
    #[error(transparent)]
    Cannon(#[from] CannonError),
    #[error(transparent)]
    Deserialize(#[from] DeserializeError),
    #[error(transparent)]
    Env(#[from] EnvError),
    #[error(transparent)]
    Execute(#[from] ExecutionError),
    #[error(transparent)]
    Schema(#[from] SchemaError),
    #[error(transparent)]
    EnvRequest(#[from] EnvRequestError),
    #[error("{0}")]
    NotFound(String),
    #[error(transparent)]
    AotCmd(#[from] AotCmdError),
}

impl_into_status_code!(ServerError, |value| match value {
    BatchReconcile(e) => e.into(),
    ContentNotFound(_) => axum::http::StatusCode::NOT_FOUND,
    Cannon(e) => e.into(),
    Deserialize(e) => e.into(),
    Env(e) => e.into(),
    Execute(e) => e.into(),
    Schema(e) => e.into(),
    EnvRequest(e) => e.into(),
    AotCmd(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    NotFound(_) => axum::http::StatusCode::NOT_FOUND,
});

impl_into_type_str!(ServerError, |value| match value {
    BatchReconcile(e) => format!("{}.{e}", value.as_ref()),
    Cannon(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Env(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Execute(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Schema(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    EnvRequest(e) => format!("{}.{}", value.as_ref(), String::from(e)),
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
    #[error("failed to open database: {0}")]
    Database(#[from] DatabaseError),
    #[error("failed to serve: {0}")]
    Serve(#[source] std::io::Error),
    #[error("failed to bind to tcp: {0}")]
    TcpBind(#[source] std::io::Error),
}

#[derive(Debug, Error, Serialize)]
#[serde(untagged)]
pub enum ActionError {
    #[error("execution timed out")]
    ExecuteStatusTimeout {
        tx_id: String,
        agent_id: Option<String>,
        retries: i32,
    },
    #[error("execution aborted")]
    ExecuteStatusAborted { tx_id: String, retries: i32 },
    #[error("execution failed")]
    ExecuteStatusFailed {
        message: String,
        tx_id: String,
        retries: i32,
    },
}

impl_into_status_code!(ActionError, |value| match value {
    ExecuteStatusTimeout { .. } => StatusCode::REQUEST_TIMEOUT,
    ExecuteStatusAborted { .. } | ExecuteStatusFailed { .. } => StatusCode::INTERNAL_SERVER_ERROR,
});

impl IntoResponse for ActionError {
    fn into_response(self) -> axum::response::Response {
        let mut json = json!(self);
        json["error"] = self.to_string().into();
        (StatusCode::from(&self), Json(&json)).into_response()
    }
}
