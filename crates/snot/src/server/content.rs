use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use super::AppState;
use crate::state::GlobalState;

pub(super) async fn init_routes(state: &GlobalState) -> Router<AppState> {
    // create storage path
    let storage_path = state.cli.path.join("storage");
    tokio::fs::create_dir_all(&storage_path)
        .await
        .expect("failed to create ledger storage path");

    Router::new()
        // the snarkOS binary
        .route_service("/snarkos", ServeFile::new("./target/release/snarkos-aot"))
        // ledger/block storage derived from tests (.tar.gz'd)
        .route_service("/storage", ServeDir::new(storage_path))
}
