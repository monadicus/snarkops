use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use http::StatusCode;
use serde_json::json;
use snops_common::state::AgentState;
use tracing::info;

use crate::state::AppState;

pub async fn start(listener: tokio::net::TcpListener, state: AppState) -> Result<()> {
    let app = Router::new()
        .route("/readyz", get(|| async { Json(json!({ "status": "ok" })) }))
        .route("/livez", get(livez))
        .with_state(Arc::clone(&state));
    info!("Starting service API on: {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn livez(State(state): State<AppState>) -> Response {
    // If the node is configured to be online, but is not online, return an error
    match state.get_agent_state().await.as_ref() {
        AgentState::Node(_, node) if node.online && !state.is_node_online() => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                   "status": "node offline",
                   "node_status": state.get_node_status().await,
                })),
            )
                .into_response()
        }
        _ => {}
    }

    if !state.is_ws_online() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "controlplane offline" })),
        )
            .into_response();
    }

    Json(json!({ "status": "ok" })).into_response()
}
