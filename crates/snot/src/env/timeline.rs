use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};

use anyhow::bail;
use futures_util::future::join_all;
use snot_common::state::AgentState;
use thiserror::Error;
use tokio::{select, sync::RwLock, task::JoinHandle};

use super::Environment;
use crate::{
    schema::timeline::{Action, ActionInstance, EventDuration},
    state::{Agent, AgentClient, AgentId, GlobalState},
};

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("an agent is offline, so the test cannot complete")]
    AgentOffline,
}

/// The tuple to pass into `reconcile_agents`.
pub type PendingAgentReconcile = (AgentId, AgentClient, AgentState);

/// Reconcile a bunch of agents at once.
pub async fn reconcile_agents<I>(iter: I, pool_mtx: &RwLock<HashMap<AgentId, Agent>>)
where
    I: Iterator<Item = PendingAgentReconcile>,
{
    use tracing::{info, warn};

    let mut handles = vec![];
    let mut agent_ids = vec![];

    for (id, client, target) in iter {
        agent_ids.push(id);
        handles.push(tokio::spawn(async move { client.reconcile(target).await }));
    }

    let num_reconciliations = handles.len();
    info!("beginning reconciliation...");
    let reconciliations = join_all(handles).await;
    info!("reconciliation complete, updating agent states...");

    let mut pool_lock = pool_mtx.write().await;
    let mut success = 0;
    for (agent_id, result) in agent_ids.into_iter().zip(reconciliations) {
        let Some(agent) = pool_lock.get_mut(&agent_id) else {
            continue;
        };

        match result {
            Ok(Ok(Ok(state))) => {
                agent.set_state(state);
                success += 1;
            }

            Ok(Err(e)) => warn!(
                "agent {} experienced a reconcilation error: {e}",
                agent.id(),
            ),

            _ => warn!(
                "agent {} failed to reconcile for an unknown reason",
                agent.id(),
            ),
        }
    }

    info!(
        "reconciliation result: {success}/{} nodes reconciled",
        num_reconciliations
    );
}

impl Environment {
    pub async fn execute(state: Arc<GlobalState>, id: usize) -> anyhow::Result<()> {
        let env = Arc::clone(match state.envs.read().await.get(&id) {
            Some(env) => env,
            None => bail!("no env with id {id}"),
        });

        // TODO: put this handle somewhere so we can terminate timeline execution
        let _handle: JoinHandle<Result<(), ExecutionError>> = tokio::spawn(async move {
            for event in env.timeline.iter() {
                let pool = state.pool.read().await;
                let mut awaiting_handles = vec![];

                if let Some(duration) = &event.duration {
                    match duration {
                        EventDuration::Time(duration) => {
                            awaiting_handles.push(tokio::spawn(tokio::time::sleep(*duration)));
                        }

                        // TODO
                        _ => unimplemented!(),
                    }
                }

                let mut pending_reconciliations: HashMap<usize, PendingAgentReconcile> =
                    HashMap::new();

                macro_rules! set_node_field {
                    ($agent:ident , $($key:ident = $val:expr),* ) => {
                        match pending_reconciliations.entry($agent.id()) {
                            Entry::Occupied(mut ent) => {
                                match ent.get_mut().2 {
                                    AgentState::Inventory => (),
                                    AgentState::Node(_, ref mut state) => {
                                        $(state.$key = $val;)*
                                    }
                                }
                            }
                            Entry::Vacant(ent) => {
                                ent.insert((
                                    $agent.id(),
                                    $agent.client_owned().ok_or(ExecutionError::AgentOffline)?,
                                    $agent.state().clone().map_node(|mut n| {
                                        $(n.$key = $val;)*
                                        n
                                    })
                                ));
                            }
                        }
                    };
                }

                for ActionInstance { action, awaited } in &event.actions.0 {
                    let handle = match action {
                        // toggle online state
                        Action::Online(targets) | Action::Offline(targets) => {
                            let online = matches!(action, Action::Online(_));

                            for agent in env.matching_agents(targets, &*pool) {
                                set_node_field!(agent, online = online);
                            }

                            // get target agents
                            // let agents = env
                            //     .matching_agents(targets, &*pool)
                            //     .map(|agent| {
                            //         agent.map_to_node_state_reconcile(|mut n|
                            // {
                            // n.online = online;
                            //             n
                            //         })
                            //     })
                            //     .collect::<Option<Vec<_>>>()
                            //     .ok_or(ExecutionError::AgentOffline)?;

                            // // reconcile each client agent
                            // let task_state = Arc::clone(&state);
                            // tokio::spawn(async move {
                            //     reconcile_agents(agents.into_iter(),
                            // &task_state.pool).await;
                            // })
                        }

                        Action::Cannon(_) => unimplemented!(),
                        Action::Height(_) => unimplemented!(),
                    };

                    if *awaited {
                        // awaiting_handles.push(handle);
                    }
                }

                drop(pool);

                // TODO: error handling
                let handles_fut = join_all(awaiting_handles.into_iter());

                // wait for the awaiting futures to complete
                match &event.timeout {
                    // apply a timeout to `handles_fut`
                    Some(timeout) => match timeout {
                        EventDuration::Time(duration) => select! {
                            _ = tokio::time::sleep(*duration) => (),
                            _ = handles_fut => (),
                        },

                        _ => unimplemented!(),
                    },

                    // no timeout, regularly await the handles
                    None => {
                        handles_fut.await;
                    }
                }
            }

            Ok(())
        });

        Ok(())
    }
}
