use std::collections::HashMap;

use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use rayon::iter::{ParallelBridge, ParallelIterator};
use serde::Serialize;
use snops_common::state::AgentState;

use super::AppState;
use crate::cli::PrometheusLocation;
pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/httpsd", get(get_httpsd))
}

#[derive(Debug, Clone, Serialize)]
pub struct StaticConfig {
    pub targets: [String; 1],
    pub labels: HashMap<&'static str, String>,
}

async fn get_httpsd(State(state): State<AppState>) -> impl IntoResponse {
    let static_configs = state
        .pool
        .iter()
        .par_bridge()
        .filter_map(|agent| {
            let agent_addr = (match (state.cli.prometheus_location, agent.has_label_str("local")) {
                // agent is external: serve its external IP
                (_, false) => agent
                    .addrs()
                    .and_then(|addrs| addrs.external.as_ref())
                    .map(ToString::to_string),

                // prometheus and agent are local: use internal IP
                (PrometheusLocation::Internal, true) => agent
                    .addrs()
                    .and_then(|addrs| addrs.internal.first())
                    .map(ToString::to_string),

                // prometheus in docker but agent is local: use host.docker.internal
                (PrometheusLocation::Docker, true) => Some(String::from("host.docker.internal")),

                // prometheus is external but agent is local: agent might not be forwarded;
                // TODO
                (PrometheusLocation::External, true) => return None,
            })?;

            let AgentState::Node(env_id, node) = agent.state() else {
                return None;
            };

            Some(StaticConfig {
                targets: [format!("{agent_addr}:{}", agent.metrics_port())],
                labels: [
                    ("env_id", env_id.to_string()),
                    ("node_key", node.node_key.to_string()),
                ]
                .into_iter()
                .collect(),
            })
        })
        .collect::<Vec<_>>();

    Json(static_configs)
}
