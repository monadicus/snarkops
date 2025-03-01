use axum::{response::IntoResponse, Json};
use http::StatusCode;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use serde_json::json;
use snops_common::{
    aot_cmds::AotCmdError, db::error::DatabaseError, events::TransactionAbortReason,
    impl_into_status_code, impl_into_type_str, schema::error::DeserializeError,
};
use thiserror::Error;

use crate::{
    apply::error::{SchemaError, StorageError},
    cannon::error::CannonError,
    env::error::{EnvError, EnvRequestError, ExecutionError},
};

#[derive(Debug, Error, strum_macros::AsRefStr)]
pub enum ServerError {
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
    #[error("{0}")]
    BadRequest(String),
    #[error(transparent)]
    AotCmd(#[from] AotCmdError),
    #[error("invalid log level: `{0}`")]
    InvalidLogLevel(String),
    #[error("failed to change log level")]
    FailedToChangeLogLevel,
    #[error(transparent)]
    RpcError(#[from] tarpc::client::RpcError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl_into_status_code!(ServerError, |value| match value {
    ContentNotFound(_) => axum::http::StatusCode::NOT_FOUND,
    Cannon(e) => e.into(),
    Deserialize(e) => e.into(),
    Env(e) => e.into(),
    Execute(e) => e.into(),
    Schema(e) => e.into(),
    EnvRequest(e) => e.into(),
    Storage(e) => e.into(),
    AotCmd(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    NotFound(_) => axum::http::StatusCode::NOT_FOUND,
    InvalidLogLevel(_) => axum::http::StatusCode::BAD_REQUEST,
    BadRequest(_) => axum::http::StatusCode::BAD_REQUEST,
    FailedToChangeLogLevel => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    RpcError(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
});

impl_into_type_str!(ServerError, |value| match value {
    Cannon(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Env(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Execute(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Schema(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    EnvRequest(e) => format!("{}.{}", value.as_ref(), String::from(e)),
    Storage(e) => format!("{}.{}", value.as_ref(), String::from(e)),
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
    ExecuteStatusAborted {
        tx_id: String,
        retries: i32,
        reason: TransactionAbortReason,
    },
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
