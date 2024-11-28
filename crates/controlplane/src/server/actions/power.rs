use std::collections::HashSet;

use axum::{
    response::{IntoResponse, Response},
    Json,
};
use snops_common::{
    action_models::WithTargets,
    node_targets::NodeTargets,
    state::{AgentId, AgentState, EnvId},
};
use tracing::info;

use super::Env;
use crate::state::{pending_reconcile_node_map, GlobalState};

async fn wait_for_nodes(
    state: &GlobalState,
    env_id: EnvId,
    nodes: NodeTargets,
    pending: Vec<(AgentId, AgentState)>,
) -> Response {
    let mut awaiting_agents = pending.iter().map(|a| a.0).collect::<HashSet<_>>();
    let node_map = pending_reconcile_node_map(pending.iter());

    // create the subscriber before updating agent states in order to
    // avoid missing any events
    use crate::events::prelude::*;
    let mut subscriber = state
        .events
        .subscribe_on(NodeTargetIs(nodes) & EnvIs(env_id) & ReconcileComplete);

    state.update_agent_states(pending).await;

    // wait at most 30 seconds for all agents to reconcile
    let expires = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    while !awaiting_agents.is_empty() {
        tokio::select! {
            _ = tokio::time::sleep_until(expires) => {
                break;
            }
            Ok(event) = subscriber.next() => {
                if let Some(agent) = event.agent {
                    awaiting_agents.remove(&agent);
                }
            }
        }
    }

    Json(node_map).into_response()
}

pub async fn online(
    Env { env, state, .. }: Env,
    Json(WithTargets { nodes, .. }): Json<WithTargets>,
) -> Response {
    info!("env {} invoked online action for {nodes}", env.id);
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
        .collect::<Vec<_>>();

    wait_for_nodes(&state, env.id, nodes, pending).await
}

pub async fn offline(
    Env { env, state, .. }: Env,
    Json(WithTargets { nodes, .. }): Json<WithTargets>,
) -> Response {
    info!("env {} invoked offline action for {nodes}", env.id);
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
        .collect::<Vec<_>>();

    wait_for_nodes(&state, env.id, nodes, pending).await
}

pub async fn reboot(env: Env, json: Json<WithTargets>) -> Response {
    let offline_res = offline(env.clone(), json.clone()).await;

    if !offline_res.status().is_success() {
        offline_res
    } else {
        online(env, json).await
    }
}
