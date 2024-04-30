use axum::{
    async_trait,
    extract::{FromRequestParts, Request},
    http::{request::Parts, Method, StatusCode, Uri},
    middleware::Next,
    response::Response,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
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
    /// HTTP status code of the response.
    http_status: u16,

    // TODO: error handling
    /// The error variant.
    error_type: Option<String>,
    /// The error data.
    error_data: Option<Value>,
}

pub async fn log_request(uri: Uri, method: Method, req_stamp: ReqStamp, res: Response) -> Response {
    // TODO: grab error data from response
    // something like:
    let err = res.extensions().get::<serde_json::Value>();
    let error_type = err.map(|e| e["type"].as_str().unwrap().to_string());
    let error_data = err.map(|e| e["error"].clone());

    let ReqStamp { uuid, time_in } = req_stamp;
    let now = Utc::now();
    let duration = now - time_in;
    let http_status = res.status().as_u16();

    let _log_line = RequestLogLine {
        uuid: uuid.to_string(),
        timestamp: now.to_rfc3339(),
        time_in: time_in.to_rfc3339(),
        duration_ms: duration.num_milliseconds(),
        http_path: uri.to_string(),
        http_method: method.to_string(),
        http_status,
        error_type,
        error_data,
    };

    // TODO: send to logging services
    // debug!("REQUEST LOG LINE:\n{}", json!(log_line));

    res
}
