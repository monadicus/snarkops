use std::collections::{hash_map::Entry, HashMap};

use axum::{
    response::{IntoResponse, Response},
    Json,
};
use snops_common::{
    action_models::{Reconfig, WithTargets},
    state::{AgentId, AgentState, InternedId},
};

use super::Env;
use crate::{
    env::PortType,
    server::error::ServerError,
    state::{pending_reconcile_node_map, PendingAgentReconcile},
};

pub async fn config(
    Env { env, state, .. }: Env,
    Json(configs): Json<Vec<WithTargets<Reconfig>>>,
) -> Response {
    let mut pending: HashMap<AgentId, PendingAgentReconcile> = HashMap::new();

    macro_rules! set_node_field {
        ($agent:ident , $($key:ident = $val:expr),* ) => {
            #[allow(unused_variables)]
            match pending.entry($agent.id()) {
                Entry::Occupied(mut ent) => {
                    match ent.get_mut().1 {
                        AgentState::Inventory => (),
                        AgentState::Node(_, ref mut n) => {
                            $({
                                let $key = &mut n.$key;
                                n.$key = $val;
                            })*
                        }
                    }
                }
                Entry::Vacant(ent) => {
                    ent.insert((
                        $agent.id(),
                        $agent.state().clone().map_node(|mut n| {
                            $({
                                let $key = &mut n.$key;
                                n.$key = $val;
                            })*
                            n
                        })
                    ));
                }
            }
        };
    }

    for WithTargets { nodes, data } in configs {
        for agent in env.matching_agents(&nodes, &state.pool) {
            if let Some(h) = data.height {
                set_node_field!(agent, height = (height.0 + 1, h));
            }

            if let Some(o) = data.online {
                set_node_field!(agent, online = o);
            }

            if let Some(p) = &data.peers {
                let p = env
                    .matching_nodes(p, &state.pool, PortType::Node)
                    .collect::<Vec<_>>();
                set_node_field!(agent, peers = p.clone());
            }

            if let Some(v) = &data.validators {
                let v = env
                    .matching_nodes(v, &state.pool, PortType::Bft)
                    .collect::<Vec<_>>();
                set_node_field!(agent, validators = v.clone());
            }

            if let Some(b) = &data.binary {
                set_node_field!(agent, binary = (*b != InternedId::default()).then_some(*b));
            }

            if let Some(k) = &data.private_key {
                let key = env.storage.sample_keysource_pk(k);
                if key.is_none() {
                    return ServerError::NotFound(format!("key source not found: `{k}`"))
                        .into_response();
                }
                set_node_field!(agent, private_key = key.clone());
            }

            // inject env fields
            if let Some(e) = &data.set_env {
                set_node_field!(
                    agent,
                    env = {
                        env.extend(e.clone());
                        std::mem::take(env)
                    }
                )
            }

            // remove env fields
            if let Some(e) = &data.del_env {
                set_node_field!(
                    agent,
                    env = {
                        env.retain(|k, _| !e.contains(k));
                        std::mem::take(env)
                    }
                )
            }
        }
    }

    let pending = pending.into_values().collect::<Vec<_>>();
    let node_map = pending_reconcile_node_map(pending.iter());

    state.update_agent_states(pending).await;
    Json(node_map).into_response()
}
