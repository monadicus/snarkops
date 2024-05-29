use axum::{response::Response, Json};

use super::{Env, WithTargets};

pub async fn online(
    Env { env, state, .. }: Env,
    Json(WithTargets { nodes, .. }): Json<WithTargets>,
) -> Response {
    let pending = env
        .matching_agents(&nodes, &state.pool)
        .map(|a| {
            a.value().filter_map_to_reconcile(|mut s| match s.online {
                true => None,
                false => {
                    s.online = false;
                    Some(s)
                }
            })
        })
        .collect::<Vec<_>>(); // TODO

    state.reconcile_agents(pending).await;

    // todo: return node keys mapped to agent ids??

    unimplemented!()
}

pub async fn offline(
    Env { env, state, .. }: Env,
    Json(WithTargets { nodes, .. }): Json<WithTargets>,
) -> Response {
    let pending = env
        .matching_agents(&nodes, &state.pool)
        .map(|a| {
            a.value().map_to_reconcile(|mut s| {
                s.online = false;
                s
            })
        })
        .collect::<Vec<_>>(); // TODO

    // ...

    state.reconcile_agents(pending).await;

    unimplemented!()
}

// pub async fn reboot(
//     params: CommonParams,
//     Json(WithTargets { nodes, .. }): Json<WithTargets>,
// ) -> Response {
//     let pending = env.matching_agents(&nodes, &state.pool).map(|a| {
//         let id = a.id();
//         let client = a.client_owned();
//         let state = a.state();
//         (id, client, state)
//     });
//     unimplemented!()
// }
