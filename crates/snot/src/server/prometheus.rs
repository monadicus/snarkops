use std::collections::HashMap;

use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use serde::Serialize;
use snot_common::state::AgentState;
use tracing::{debug, info};

use super::AppState;
use crate::{env::EnvPeer, state::AgentAddrs};
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

#[axum::debug_handler]
async fn get_httpsd(State(state): State<AppState>) -> impl IntoResponse {
    let mut prom_httpsd = state.prom_httpsd.lock().await;

    let static_configs = match &*prom_httpsd {
        // use the cached response
        HttpsdResponse::Clean(static_configs) => static_configs.to_owned(),

        // recompute the response and save it
        HttpsdResponse::Dirty => {
            debug!("httpsd response is dirty, regenerating...");
            let pool = state.pool.read().await;
            let envs = state.envs.read().await;

            let mut static_configs = vec![];

            for (agent_id, agent) in pool.iter() {
                let Some(agent_addr) = agent.addrs().and_then(AgentAddrs::usable) else {
                    continue;
                };

                match agent.state() {
                    AgentState::Node(env_id, _) => {
                        // get the environment this agent belongs to
                        let Some(env) = envs.get(env_id) else {
                            continue;
                        };

                        // get the node key that corresponds to this agent
                        let Some(node_key) =
                            env.node_map.get_by_right(&EnvPeer::Internal(*agent_id))
                        else {
                            continue;
                        };

                        info!("agent {} addrs: {:#?}", agent_id, agent.addrs());

                        static_configs.push(StaticConfig {
                            // targets: [format!("{agent_addr}:9000")], // TODO: metrics port
                            targets: ["host.docker.internal:9000".into()], // TODO: don't hard-code this :(
                            labels: [
                                ("env_id".into(), env_id.to_string()),
                                ("agent_id".into(), node_key.to_string()),
                            ]
                            .into_iter()
                            .collect(),
                        });
                    }
                    _ => (),
                }
            }

            *prom_httpsd = HttpsdResponse::Clean(static_configs.to_owned());

            static_configs
        }
    };

    Json(static_configs)
}
