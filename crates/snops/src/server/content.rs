use axum::{
    middleware,
    response::{IntoResponse, Response},
    Router,
};
use http::{StatusCode, Uri};
use tower_http::services::{ServeDir, ServeFile};

use super::AppState;
use crate::{
    schema::storage::{DEFAULT_AGENT_BIN, DEFAULT_AOT_BIN},
    server::error::ServerError,
    state::GlobalState,
};

async fn not_found(uri: Uri, res: Response) -> Response {
    match res.status() {
        StatusCode::NOT_FOUND => {
            let path = uri.path();
            let content = path.split('/').last().unwrap();
            ServerError::ContentNotFound(content.to_owned()).into_response()
        }
        _ => res,
    }
}

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
        // TODO: change this to be more restrictive
        .nest_service("/storage", ServeDir::new(storage_path))
        .layer(middleware::map_response(not_found))
    // TODO: ServeFile for all files by /storage/<storageid>/binaries/<binaryId>
}
