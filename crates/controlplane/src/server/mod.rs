use std::{net::SocketAddr, sync::Arc};

use axum::{middleware, routing::get, Extension, Router};

use self::error::StartError;
use crate::{
    logging::{log_request, req_stamp},
    state::GlobalState,
};

pub mod actions;
mod api;
mod content;
pub mod error;
pub mod jwt;
pub mod models;
pub mod prometheus;
mod rpc;
mod websocket;

pub async fn start(state: Arc<GlobalState>, socket_addr: SocketAddr) -> Result<(), StartError> {
    let app = Router::new()
        .route("/agent", get(websocket::agent_ws_handler))
        .nest("/api/v1", api::routes())
        .nest("/prometheus", prometheus::routes())
        .nest("/content", content::init_routes(&state).await)
        .with_state(Arc::clone(&state))
        .layer(Extension(state))
        .layer(middleware::map_response(log_request))
        .layer(middleware::from_fn(req_stamp));

    let listener = tokio::net::TcpListener::bind(socket_addr)
        .await
        .map_err(StartError::TcpBind)?;

    axum::serve(listener, app)
        .await
        .map_err(StartError::Serve)?;

    Ok(())
}
