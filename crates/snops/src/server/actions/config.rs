use std::collections::{hash_map::Entry, HashMap};

use axum::{
    response::{IntoResponse, Response},
    Json,
};
use snops_common::{
    action_models::{Reconfig, WithTargets},
    state::{AgentId, AgentState},
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
                    match ent.get_mut().2 {
                        AgentState::Inventory => (),
                        AgentState::Node(_, ref mut n) => {
                            $({
                                let $key = &n.$key;
                                n.$key = $val;
                            })*
                        }
                    }
                }
                Entry::Vacant(ent) => {
                    ent.insert((
                        $agent.id(),
                        $agent.client_owned(),
                        $agent.state().clone().map_node(|mut n| {
                            $({
                                let $key = &n.$key;
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
                let h = h.into();
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
        }
    }

    let pending = pending.into_values().collect::<Vec<_>>();
    let node_map = pending_reconcile_node_map(pending.iter());

    let res = state
        .reconcile_agents(pending)
        .await
        .map_err(ServerError::from);

    match res {
        Ok(_) => Json(node_map).into_response(),
        e => e.into_response(),
    }
}
