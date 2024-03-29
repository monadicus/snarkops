use axum::{
    async_trait,
    extract::{FromRequestParts, Request},
    http::{request::Parts, Method, StatusCode, Uri},
    middleware::Next,
    response::Response,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use tracing::debug;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ReqStamp {
    pub uuid: Uuid,
    pub time_in: DateTime<Utc>,
}

pub async fn req_stamp(mut req: Request, next: Next) -> Response {
    let time_in = Utc::now();
    let uuid = Uuid::new_v4();

    req.extensions_mut().insert(ReqStamp { uuid, time_in });

    next.run(req).await
}

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for ReqStamp {
    // TODO replace with our own error type that implements IntoResponse
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> core::result::Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<ReqStamp>()
            .cloned()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

#[derive(Serialize)]
struct RequestLogLine {
    /// Unique request identifier.
    uuid: String,
    /// Timestamp(rfc3339) of the log line.
    timestamp: String,
    /// Timestamp(rfc3339) of the request.
    time_in: String,
    /// Duration of the request in milliseconds.
    duration_ms: i64,

    /// HTTP path of the request.
    http_path: String,
    /// HTTP method of the request.
    http_method: String,

    // TODO: error handling
    /// The error variant.
    error_type: Option<String>,
    /// The error data.
    error_data: Option<Value>,
}

pub async fn log_request(uri: Uri, method: Method, req_stamp: ReqStamp, res: Response) -> Response {
    // TODO: grab error data from response
    // something like:
    // res.extensions_mut().get::<OurErrorType>();

    let ReqStamp { uuid, time_in } = req_stamp;
    let now = Utc::now();
    let duration = now - time_in;

    let log_line = RequestLogLine {
        uuid: uuid.to_string(),
        timestamp: now.to_rfc3339(),
        time_in: time_in.to_rfc3339(),
        duration_ms: duration.num_milliseconds(),
        http_path: uri.to_string(),
        http_method: method.to_string(),
        error_type: None,
        error_data: None,
    };

    // TODO: send to logging services
    debug!("REQUEST LOG LINE:\n{}", json!(log_line));

    res
}
