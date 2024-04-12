use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use super::AppState;
use crate::{
    schema::storage::{DEFAULT_AGENT_BIN, DEFAULT_AOT_BIN},
    state::GlobalState,
};

pub(super) async fn init_routes(state: &GlobalState) -> Router<AppState> {
    // create storage path
    let storage_path = state.cli.path.join("storage");
    tracing::debug!("storage path: {:?}", storage_path);
    tokio::fs::create_dir_all(&storage_path)
        .await
        .expect("failed to create ledger storage path");

    Router::new()
        // the snarkOS binary
        .route_service("/snarkos", ServeFile::new(DEFAULT_AOT_BIN.clone()))
        // the agent binary
        .route_service("/agent", ServeFile::new(DEFAULT_AGENT_BIN.clone()))
        // ledger/block storage derived from tests (.tar.gz'd)
        .nest_service("/storage", ServeDir::new(storage_path))
}
