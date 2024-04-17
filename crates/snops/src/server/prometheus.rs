use std::{collections::HashMap, fmt::Write};

use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use serde::Serialize;
use snops_common::state::AgentState;
use tracing::debug;

use super::AppState;
use crate::{cli::PrometheusLocation, env::EnvPeer};
pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/httpsd", get(get_httpsd))
}

#[derive(Debug, Clone, Serialize)]
pub struct StaticConfig {
    pub targets: [String; 1],
    pub labels: HashMap<String, String>,
}

/// Caching container for the Prometheus HTTP service discovery response. Marked
/// 'dirty' when environment agents are reallocated.
#[derive(Debug, Clone, Default)]
pub enum HttpsdResponse {
    #[default]
    Dirty,
    Clean(Vec<StaticConfig>),
}

impl HttpsdResponse {
    pub fn set_dirty(&mut self) {
        *self = Self::Dirty;
    }
}

async fn get_httpsd(State(state): State<AppState>) -> impl IntoResponse {
    let mut prom_httpsd = state.prom_httpsd.lock().await;

    let static_configs = match &*prom_httpsd {
        // use the cached response
        HttpsdResponse::Clean(static_configs) => static_configs.to_owned(),

        // recompute the response and save it
        HttpsdResponse::Dirty => {
            debug!("httpsd response is dirty, regenerating...");
            let mut static_configs = vec![];

            for agent in state.pool.iter() {
                let Some(mut agent_addr) =
                    (match (state.cli.prometheus_location, agent.has_label_str("local")) {
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
                        (PrometheusLocation::Docker, true) => {
                            Some(String::from("host.docker.internal"))
                        }

                        // prometheus is external but agent is local: agent might not be forwarded;
                        // TODO
                        (PrometheusLocation::External, true) => continue,
                    })
                else {
                    continue;
                };

                match agent.state() {
                    AgentState::Node(env_id, _) => {
                        // get the environment this agent belongs to
                        let Some(env) = state.get_env(*env_id) else {
                            continue;
                        };

                        // get the node key that corresponds to this agent
                        let Some(node_key) =
                            env.node_map.get_by_right(&EnvPeer::Internal(agent.id()))
                        else {
                            continue;
                        };

                        agent_addr
                            .write_fmt(format_args!(":{}", agent.metrics_port()))
                            .unwrap();

                        static_configs.push(StaticConfig {
                            targets: [agent_addr],
                            labels: [
                                ("env_id".into(), env_id.to_string()),
                                ("agent_id".into(), node_key.to_string()),
                            ]
                            .into_iter()
                            .collect(),
                        });
                    }

                    _ => {
                        // future-proofing; this comment also disables the
                        // clippy lint
                    }
                }
            }

            *prom_httpsd = HttpsdResponse::Clean(static_configs.to_owned());

            static_configs
        }
    };

    Json(static_configs)
}
