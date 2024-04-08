use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{atomic::Ordering, Arc},
};

use futures_util::future::join_all;
use prometheus_http_query::response::Data;
use promql_parser::label::{MatchOp, Matcher};
use snops_common::state::{AgentId, AgentState};
use tokio::{select, sync::RwLock, task::JoinHandle};
use tracing::{debug, error, info, warn};

use super::{
    error::{BatchReconcileError, ExecutionError},
    EnvError, Environment,
};
use crate::{
    cannon::{
        sink::TxSink,
        source::{QueryTarget, TxSource},
        CannonInstance,
    },
    schema::{
        outcomes::PromQuery,
        timeline::{Action, ActionInstance, EventDuration},
    },
    state::{Agent, AgentClient, GlobalState},
};

/// The tuple to pass into `reconcile_agents`.
pub type PendingAgentReconcile = (AgentId, AgentClient, AgentState);

/// Reconcile a bunch of agents at once.
pub async fn reconcile_agents<I>(
    iter: I,
    pool_mtx: &RwLock<HashMap<AgentId, Agent>>,
) -> Result<(), BatchReconcileError>
where
    I: Iterator<Item = PendingAgentReconcile>,
{
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
            Ok(Ok(Err(e))) => error!(
                "agent {} experienced a reconcilation error: {e}",
                agent.id(),
            ),

            Ok(Err(e)) => error!("agent {} experienced a rpc error: {e}", agent.id(),),
            Err(e) => error!("agent {} experienced a join error: {e}", agent.id(),),
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
    pub async fn execute(state: Arc<GlobalState>, env_id: usize) -> Result<(), EnvError> {
        let env = Arc::clone(
            state
                .envs
                .read()
                .await
                .get(&env_id)
                .ok_or_else(|| ExecutionError::EnvNotFound(env_id))?,
        );

        info!(
            "starting timeline playback for env {env_id} with {} events",
            env.timeline.len()
        );

        let handle_lock_env = Arc::clone(&env);
        let mut handle_lock = handle_lock_env.timeline_handle.lock().await;

        // abort if timeline is already being executed
        if !handle_lock
            .as_ref()
            .map(JoinHandle::is_finished)
            .unwrap_or(true)
        {
            Err(ExecutionError::TimelineAlreadyStarted)?;
        }

        *handle_lock = Some(tokio::spawn(async move {
            for event in env.timeline.iter() {
                debug!("next event in timeline {event:?}");
                let pool = state.pool.read().await;

                // task handles that must be awaited for this timeline event
                let mut awaiting_handles: Vec<tokio::task::JoinHandle<Result<(), ExecutionError>>> =
                    vec![];

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
                let mut pending_reconciliations: HashMap<AgentId, PendingAgentReconcile> =
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

                        Action::Cannon(cannons) => {
                            for cannon in cannons.iter() {
                                let cannon_id = env.cannons_counter.fetch_add(1, Ordering::Relaxed);
                                let Some((mut source, mut sink)) =
                                    env.cannon_configs.get(&cannon.name).cloned()
                                else {
                                    return Err(ExecutionError::UnknownCannon(cannon.name.clone()));
                                };

                                // override the query and target if they are specified
                                if let (Some(q), TxSource::RealTime { query, .. }) =
                                    (&cannon.query, &mut source)
                                {
                                    *query = QueryTarget::Node(q.clone());
                                };

                                if let (Some(t), TxSink::RealTime { target, .. }) =
                                    (&cannon.target, &mut sink)
                                {
                                    *target = t.clone();
                                };
                                let count = cannon.count;

                                let (mut instance, rx) = CannonInstance::new(
                                    state.clone(),
                                    cannon_id,
                                    env.clone(),
                                    source,
                                    sink,
                                    count,
                                )
                                .await
                                .map_err(ExecutionError::Cannon)?;

                                if *awaited {
                                    let ctx = instance.ctx().unwrap();
                                    let env = env.clone();

                                    // debug!("instance started await mode");
                                    awaiting_handles.push(tokio::task::spawn(async move {
                                        let res = ctx.spawn(rx).await;

                                        // remove the cannon after the task is complete
                                        env.cannons.write().await.remove(&cannon_id);
                                        res.map_err(ExecutionError::Cannon)
                                    }));
                                } else {
                                    instance
                                        .spawn_local(rx)
                                        .await
                                        .map_err(ExecutionError::Cannon)?;
                                }

                                // insert the cannon
                                env.cannons.write().await.insert(cannon_id, instance);
                            }
                        }
                        Action::Height(_) => unimplemented!(),
                    };
                }

                drop(pool);

                // if there are any pending reconciliations,
                if !pending_reconciliations.is_empty() {
                    // reconcile all nodes
                    let task_state = Arc::clone(&state);
                    let reconcile_handle = tokio::spawn(async move {
                        reconcile_agents(pending_reconciliations.into_values(), &task_state.pool)
                            .await?;
                        Ok(())
                    });

                    // await the reconciliation if any of the actions were `.await`
                    if reconcile_async {
                        awaiting_handles.push(reconcile_handle);
                    }
                }

                let handles_fut = join_all(awaiting_handles.into_iter());

                // wait for the awaiting futures to complete
                let handles_result = match &event.timeout {
                    // apply a timeout to `handles_fut`
                    Some(timeout) => match timeout {
                        EventDuration::Time(timeout_duration) => select! {
                            _ = tokio::time::sleep(*timeout_duration) => continue,
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
                        Ok(e) => return e,
                        Err(e) => return Err(ExecutionError::Join(e)),
                    }
                }
            }

            info!("------------------------------------------");
            info!("playback of environment timeline completed");
            info!("------------------------------------------");

            // perform outcome validation
            if let Some(prometheus) = &*state.prometheus {
                for (outcome_name, outcome) in env.outcomes.iter() {
                    let Some(mut query) = outcome
                        .query
                        .as_ref()
                        .or_else(|| PromQuery::builtin(&outcome_name))
                        .cloned()
                    else {
                        warn!("unrecognized metric name (no built-in query found)");
                        continue;
                    };

                    // inject env ID matchers into the PromQL query
                    query.add_matchers(&[Matcher {
                        op: MatchOp::Equal,
                        name: String::from("env_id"),
                        value: env_id.to_string(),
                    }]);

                    // TODO: store pass/fails in environment

                    let query_response = prometheus.query(query.into_inner()).get().await;
                    match query_response {
                        Ok(result) => {
                            let value = match result.data() {
                                Data::Scalar(sample) => sample.value(),
                                Data::Vector(vector) => match vector.last() {
                                    Some(item) => item.sample().value(),
                                    None => {
                                        warn!("empty vector response from prometheus");
                                        continue;
                                    }
                                },
                                _ => {
                                    warn!("unsupported prometheus query response");
                                    continue;
                                }
                            };
                            let message = outcome.validation.show_validation(value);
                            info!("OUTCOME {outcome_name}: {message}");
                        }

                        Err(e) => {
                            error!("failed to validate outcome {outcome_name}: {e}");
                        }
                    }
                }
            }

            Ok(())
        }));

        Ok(())
    }
}
