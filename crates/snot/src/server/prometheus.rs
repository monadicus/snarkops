use std::{collections::HashMap, net::IpAddr};

use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use serde::Serialize;
use snot_common::state::AgentState;

use super::AppState;
use crate::state::AgentAddrs;
pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/httpsd", get(get_httpsd))
}

#[derive(Debug, Clone, Serialize)]
pub struct StaticConfig {
    pub targets: [IpAddr; 1],
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
    let pool = state.pool.read().await;

    let mut prom_httpsd = state.prom_httpsd.lock().unwrap();
    let static_configs = match &*prom_httpsd {
        // use the cached response
        HttpsdResponse::Clean(static_configs) => static_configs.to_owned(),

        // recompute the response and save it
        HttpsdResponse::Dirty => {
            let mut static_configs = vec![];

            for (agent_id, agent) in pool.iter() {
                let Some(agent_addr) = agent.addrs().and_then(AgentAddrs::usable) else {
                    continue;
                };

                match agent.state() {
                    AgentState::Node(env_id, _) => {
                        static_configs.push(StaticConfig {
                            targets: [agent_addr],
                            labels: [
                                ("env_id".into(), env_id.to_string()),
                                ("agent_id".into(), agent_id.to_string()),
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
