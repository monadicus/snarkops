use std::str::FromStr;

use axum::{
    extract::{Path, Request, State},
    middleware,
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use http::{StatusCode, Uri};
use snops_common::{
    binaries::{BinaryEntry, BinarySource},
    state::{InternedId, NetworkId, StorageId},
};
use tower::Service;
use tower_http::services::ServeFile;

use crate::{
    schema::{
        error::StorageError,
        storage::{DEFAULT_AGENT_BINARY, DEFAULT_AOT_BINARY},
    },
    server::error::ServerError,
    state::{AppState, GlobalState},
    unwrap_or_not_found,
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
        .route_service(
            "/snarkos",
            get(|req: Request| respond_from_entry(InternedId::default(), &DEFAULT_AOT_BINARY, req))
                .head(|req: Request| {
                    respond_from_entry(InternedId::default(), &DEFAULT_AOT_BINARY, req)
                }),
        )
        // the agent binary
        .route_service(
            "/agent",
            get(|req: Request| {
                respond_from_entry(
                    InternedId::from_str("agent").unwrap(),
                    &DEFAULT_AGENT_BINARY,
                    req,
                )
            }),
        )
        // ledger/block storage derived from tests (.tar.gz'd)
        .route("/storage/:network/:storage_id/:file", get(serve_file))
        .route(
            "/storage/:network/:storage_id/binaries/:id",
            get(serve_binary).head(serve_binary),
        )
        .layer(middleware::map_response(not_found))
}

/// Serve a binary from the storage or a redirect to the binary
async fn serve_binary(
    Path((network, storage_id, binary_id)): Path<(NetworkId, StorageId, InternedId)>,
    State(state): State<AppState>,
    req: Request,
) -> Response {
    let storage = unwrap_or_not_found!(state.storage.get(&(network, storage_id))).clone();

    match storage.resolve_binary_entry(binary_id) {
        Ok((id, entry)) => respond_from_entry(id, entry, req).await,
        Err(e) => ServerError::from(e).into_response(),
    }
}

/// Given a binary entry, respond with the binary or a redirect to the binary
async fn respond_from_entry(id: InternedId, entry: &BinaryEntry, req: Request) -> Response {
    match &entry.source {
        BinarySource::Url(url) => Redirect::temporary(url.as_str()).into_response(),
        BinarySource::Path(file) if !file.exists() => {
            ServerError::from(StorageError::BinaryFileMissing(id, file.clone())).into_response()
        }
        BinarySource::Path(file) => ServeFile::new(file).call(req).await.into_response(),
    }
}

async fn serve_file(
    Path((network, storage_id, file)): Path<(NetworkId, StorageId, String)>,
    State(state): State<AppState>,
    req: Request,
) -> Response {
    let storage = unwrap_or_not_found!(state.storage.get(&(network, storage_id))).clone();

    match file.as_str() {
        // ensure genesis is only served if native genesis is disabled
        "genesis.block" => {
            if storage.native_genesis {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
        // otherwise, return a 404
        _ => return StatusCode::NOT_FOUND.into_response(),
    }

    let file_path = storage.path(&state).join(&file);

    // ensure the file exists
    if !file_path.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    // serve the file
    ServeFile::new(file_path).call(req).await.into_response()
}
