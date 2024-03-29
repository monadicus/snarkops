use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};

use anyhow::bail;
use futures_util::future::join_all;
use snot_common::state::AgentState;
use thiserror::Error;
use tokio::{select, sync::RwLock, task::JoinError};
use tracing::info;

use super::Environment;
use crate::{
    schema::timeline::{Action, ActionInstance, EventDuration},
    state::{Agent, AgentClient, AgentId, GlobalState},
};

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("an agent is offline, so the test cannot complete")]
    AgentOffline,
    #[error("reconcilation failed: {0}")]
    Reconcile(#[from] BatchReconcileError),
    #[error("join error: {0}")]
    Join(#[from] JoinError),
}

/// The tuple to pass into `reconcile_agents`.
pub type PendingAgentReconcile = (AgentId, AgentClient, AgentState);

#[derive(Debug, Error)]
#[error("batch reconciliation failed with {failures} failed reconciliations")]
pub struct BatchReconcileError {
    pub failures: usize,
}

/// Reconcile a bunch of agents at once.
pub async fn reconcile_agents<I>(
    iter: I,
    pool_mtx: &RwLock<HashMap<AgentId, Agent>>,
) -> Result<(), BatchReconcileError>
where
    I: Iterator<Item = PendingAgentReconcile>,
{
    use tracing::warn;

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

    if success == num_reconciliations {
        Ok(())
    } else {
        Err(BatchReconcileError {
            failures: num_reconciliations - success,
        })
    }
}

impl Environment {
    pub async fn execute(state: Arc<GlobalState>, id: usize) -> anyhow::Result<()> {
        let env = Arc::clone(match state.envs.read().await.get(&id) {
            Some(env) => env,
            None => bail!("no env with id {id}"),
        });

        let handle_lock_env = Arc::clone(&env);
        let mut handle_lock = handle_lock_env.timeline_handle.lock().await;

        // abort if timeline is already being executed
        match &*handle_lock {
            Some(handle) if !handle.is_finished() => {
                bail!("environment timeline is already being executed")
            }
            _ => (),
        }

        *handle_lock = Some(tokio::spawn(async move {
            for event in env.timeline.iter() {
                let pool = state.pool.read().await;

                // task handles that must be awaited for this timeline event
                let mut awaiting_handles = vec![];

                // add a duration sleep if a duration was specified
                if let Some(duration) = &event.duration {
                    match duration {
                        &EventDuration::Time(duration) => {
                            awaiting_handles.push(tokio::spawn(async move {
                                tokio::time::sleep(duration).await;
                                Ok(())
                            }));
                        }

                        // TODO
                        _ => unimplemented!(),
                    }
                }

                // whether or not to reconcile asynchronously (if any of the reconcile actions
                // are awaited)
                let mut reconcile_async = false;

                // the pending reconciliations
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
                    match action {
                        // toggle online state
                        Action::Online(targets) | Action::Offline(targets) => {
                            if *awaited {
                                reconcile_async = true;
                            }

                            let online = matches!(action, Action::Online(_));

                            for agent in env.matching_agents(targets, &pool) {
                                set_node_field!(agent, online = online);
                            }
                        }

                        Action::Cannon(_) => unimplemented!(),
                        Action::Height(_) => unimplemented!(),
                    };
                }

                drop(pool);

                let task_state = Arc::clone(&state);
                let reconcile_handle = tokio::spawn(async move {
                    reconcile_agents(pending_reconciliations.into_values(), &task_state.pool).await
                });

                // await the reconciliation if any of the actions were `.await`
                if reconcile_async {
                    awaiting_handles.push(reconcile_handle);
                }

                let handles_fut = join_all(awaiting_handles.into_iter());

                // wait for the awaiting futures to complete
                let handles_result = match &event.timeout {
                    // apply a timeout to `handles_fut`
                    Some(timeout) => match timeout {
                        EventDuration::Time(duration) => select! {
                            _ = tokio::time::sleep(*duration) => continue,
                            res = handles_fut => res,
                        },

                        _ => unimplemented!(),
                    },

                    // no timeout, regularly await the handles
                    None => handles_fut.await,
                };

                for result in handles_result.into_iter() {
                    match result {
                        Ok(Ok(())) => (),
                        Ok(Err(e)) => return Err(ExecutionError::Reconcile(e)),
                        Err(e) => return Err(ExecutionError::Join(e)),
                    }
                }
            }

            info!("------------------------------------------");
            info!("playback of environment timeline completed");
            info!("------------------------------------------");

            Ok(())
        }));

        Ok(())
    }
}
