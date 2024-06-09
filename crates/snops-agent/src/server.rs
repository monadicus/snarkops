use std::sync::Arc;

use anyhow::Result;
use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use http::StatusCode;
use snops_common::state::snarkos_status::{SnarkOSBlockInfo, SnarkOSStatus};
use tarpc::context;

use crate::state::AppState;

pub async fn start(listener: tokio::net::TcpListener, state: AppState) -> Result<()> {
    let app = Router::new()
        .route("/api/v1/block", post(post_block_info))
        .route("/api/v1/status", post(post_node_status))
        .with_state(Arc::clone(&state));

    axum::serve(listener, app).await?;

    Ok(())
}

async fn post_block_info(
    State(state): State<AppState>,
    Json(SnarkOSBlockInfo {
        height,
        state_root,
        block_hash,
        block_timestamp,
    }): Json<SnarkOSBlockInfo>,
) -> impl IntoResponse {
    match state
        .client
        .post_block_status(
            context::current(),
            height,
            block_timestamp,
            state_root,
            block_hash,
        )
        .await
    {
        Ok(()) => StatusCode::OK,
        Err(err) => {
            tracing::error!("failed to post block status: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn post_node_status(
    State(state): State<AppState>,
    Json(status): Json<SnarkOSStatus>,
) -> impl IntoResponse {
    match state
        .client
        .post_node_status(context::current(), status.into())
        .await
    {
        Ok(()) => StatusCode::OK,
        Err(err) => {
            tracing::error!("failed to post node status: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
