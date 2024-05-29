use std::collections::HashMap;

use axum::{
    response::{IntoResponse, Response},
    Json,
};

use super::{Env, WithTargets};
use crate::{server::error::ServerError, state::Agent};

pub async fn online(
    Env { env, state, .. }: Env,
    Json(WithTargets { nodes, .. }): Json<WithTargets>,
) -> Response {
    let mut node_map = HashMap::new();
    let pending = env
        .matching_agents(&nodes, &state.pool)
        .filter_map(|a| {
            let agent: &Agent = a.value();
            agent.filter_map_to_reconcile(|mut s| match s.online {
                true => None,
                false => {
                    s.online = true;

                    Some(s)
                }
            })
        })
        .inspect(|(id, _, _)| {
            node_map.insert(env.get_node_key_by_agent(*id).unwrap(), *id);
        })
        .collect::<Vec<_>>(); // TODO

    let res = state
        .reconcile_agents(pending)
        .await
        .map_err(ServerError::from);

    match res {
        Ok(_) => Json(node_map).into_response(),
        e => e.into_response(),
    }
}

pub async fn offline(
    Env { env, state, .. }: Env,
    Json(WithTargets { nodes, .. }): Json<WithTargets>,
) -> Response {
    let mut node_map = HashMap::new();
    let pending = env
        .matching_agents(&nodes, &state.pool)
        .filter_map(|a| {
            let agent: &Agent = a.value();
            agent.filter_map_to_reconcile(|mut s| match s.online {
                false => None,
                true => {
                    s.online = false;
                    Some(s)
                }
            })
        })
        .inspect(|(id, _, _)| {
            node_map.insert(env.get_node_key_by_agent(*id).unwrap(), *id);
        })
        .collect::<Vec<_>>(); // TODO

    // ...

    let res = state
        .reconcile_agents(pending)
        .await
        .map_err(ServerError::from);

    match res {
        Ok(_) => Json(node_map).into_response(),
        e => e.into_response(),
    }
}

pub async fn reboot(
    Env { env, state, .. }: Env,
    Json(WithTargets { nodes, .. }): Json<WithTargets>,
) -> Response {
    let mut node_map = HashMap::new();
    let pending = env
        .matching_agents(&nodes, &state.pool)
        .map(|a| {
            let agent: &Agent = a.value();
            node_map.insert(env.get_node_key_by_agent(a.id()).unwrap(), a.id());
            agent.map_to_reconcile(|mut s| {
                s.online = true;
                s
            })
        })
        .collect::<Vec<_>>(); // TODO

    let res = state
        .reconcile_agents(pending)
        .await
        .map_err(ServerError::from);

    match res {
        Ok(_) => Json(node_map).into_response(),
        e => e.into_response(),
    }
}
