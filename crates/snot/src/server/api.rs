use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use serde_json::json;

use super::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/foo", get(|| async { "bar" }))
        .route("/agents", get(get_agents))
    // .route("/test", post(post_test))
}

async fn get_agents(State(state): State<AppState>) -> impl IntoResponse {
    // TODO: return actual relevant info about agents
    Json(json!({ "count": state.pool.read().await.len() }))
}

// async fn post_test(State(state): State<AppState>) -> impl IntoResponse {
//     // just to test, this sets the desired state of all nodes to online
// clients     let mut pool = state.pool.write().await;

// let desired_state = ConfigRequest::new()
//     .with_online(true)
//     .with_type(Some(NodeType::Client));

//     for agent in pool.values_mut() {
//         agent.set_state(desired_state.to_owned()).await.unwrap();
//     }

//     StatusCode::OK
// }
