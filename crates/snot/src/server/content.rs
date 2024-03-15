use axum::Router;
use tower_http::services::ServeFile;

use super::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new().route_service(
        "/snarkos",
        ServeFile::new("../snarkos/target/release/snarkos"),
    )
}
