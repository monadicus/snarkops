use std::collections::HashSet;

use axum::{
    response::{IntoResponse, Response},
    Json,
};
use snops_common::action_models::WithTargets;

use super::Env;
use crate::state::pending_reconcile_node_map;

pub async fn online(
    Env { env, state, .. }: Env,
    Json(WithTargets { nodes, .. }): Json<WithTargets>,
) -> Response {
    let pending = env
        .matching_agents(&nodes, &state.pool)
        .filter_map(|a| {
            a.value().filter_map_to_reconcile(|mut s| {
                (!s.online).then(|| {
                    s.online = true;
                    s
                })
            })
        })
        .collect::<Vec<_>>(); // TODO

    let mut awaiting_agents = pending.iter().map(|a| a.0).collect::<HashSet<_>>();

    let node_map = pending_reconcile_node_map(pending.iter());

    state.update_agent_states(pending).await;

    use crate::events::prelude::*;
    let mut subscriber = state
        .events
        .subscribe_on(NodeTargetIs(nodes) & EnvIs(env.id) & ReconcileComplete);

    while !awaiting_agents.is_empty() {
        // TODO: expire after some time
        if let Ok(event) = subscriber.next().await {
            if let Some(agent) = event.agent {
                awaiting_agents.remove(&agent);
            }
        }
    }

    Json(node_map).into_response()
}

pub async fn offline(
    Env { env, state, .. }: Env,
    Json(WithTargets { nodes, .. }): Json<WithTargets>,
) -> Response {
    let pending = env
        .matching_agents(&nodes, &state.pool)
        .filter_map(|a| {
            a.value().filter_map_to_reconcile(|mut s| {
                s.online.then(|| {
                    s.online = false;
                    s
                })
            })
        })
        .collect::<Vec<_>>(); // TODO

    let node_map = pending_reconcile_node_map(pending.iter());

    state.update_agent_states(pending).await;

    Json(node_map).into_response()
}

pub async fn reboot(env: Env, json: Json<WithTargets>) -> Response {
    let offline_res = offline(env.clone(), json.clone()).await;

    // TODO: wait for nodes to reconcile offline

    if !offline_res.status().is_success() {
        offline_res
    } else {
        online(env, json).await
    }
}
