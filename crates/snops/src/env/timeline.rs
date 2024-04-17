use std::{
    collections::{hash_map::Entry, HashMap},
    str::FromStr,
    sync::Arc,
};

use futures_util::future::join_all;
use prometheus_http_query::response::Data;
use promql_parser::label::{MatchOp, Matcher};
use rand::RngCore;
use snops_common::state::{AgentId, AgentState, CannonId, EnvId, TimelineId};
use tokio::{select, task::JoinHandle};
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
    db::document::DbDocument,
    env::PortType,
    schema::{
        outcomes::PromQuery,
        timeline::{Action, ActionInstance, EventDuration},
    },
    state::{AgentClient, GlobalState},
};

/// The tuple to pass into `reconcile_agents`.
pub type PendingAgentReconcile = (AgentId, AgentClient, AgentState);

/// Reconcile a bunch of agents at once.
pub async fn reconcile_agents<I>(state: &GlobalState, iter: I) -> Result<(), BatchReconcileError>
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

    let mut success = 0;
    for (agent_id, result) in agent_ids.into_iter().zip(reconciliations) {
        let Some(mut agent) = state.pool.get_mut(&agent_id) else {
            continue;
        };

        match result {
            Ok(Ok(Ok(agent_state))) => {
                agent.set_state(agent_state);
                if let Err(e) = agent.save(&state.db, agent_id) {
                    error!("failed to save agent {agent_id} to the database: {e}");
                }

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
    pub async fn execute(
        state: Arc<GlobalState>,
        env_id: EnvId,
        timeline_id: TimelineId,
    ) -> Result<(), EnvError> {
        let env = state
            .get_env(env_id)
            .ok_or_else(|| ExecutionError::EnvNotFound(env_id))?;

        let timeline = env
            .timelines
            .get(&timeline_id)
            .ok_or_else(|| ExecutionError::TimelineNotFound(env_id, timeline_id))?
            .clone();

        info!(
            "starting timeline {timeline_id} playback for env {env_id} with {} events",
            timeline.len()
        );

        // TODO do we need to move these locks now to a new struct inside the timelines
        // hashmap?
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
            for event in timeline.iter() {
                debug!("next event in timeline {event:?}");
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
                        #[allow(unused_variables)]
                        match pending_reconciliations.entry($agent.id()) {
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
                                    $agent.client_owned().ok_or(ExecutionError::AgentOffline)?,
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

                for ActionInstance { action, awaited } in &event.actions.0 {
                    match action {
                        // toggle online state
                        Action::Online(targets) | Action::Offline(targets) => {
                            if *awaited {
                                reconcile_async = true;
                            }

                            let o = matches!(action, Action::Online(_));

                            for agent in env.matching_agents(targets, &state.pool) {
                                set_node_field!(agent, online = o);
                            }
                        }

                        Action::Cannon(cannons) => {
                            for cannon in cannons.iter() {
                                let counter = rand::thread_rng().next_u32();
                                let cannon_id =
                                    CannonId::from_str(&format!("{}-{counter}", cannon.name))
                                        // there is a small chance that the cannon's name is at the
                                        // length limit, so this will force the cannon to be renamed
                                        // to 'cannon-N'
                                        .unwrap_or_else(|_| {
                                            CannonId::from_str(&format!("cannon-{counter}"))
                                                .expect("cannon id failed to parse")
                                        });

                                let Some((mut source, mut sink)) =
                                    env.cannon_configs.get(&cannon.name).map(|c| c.clone())
                                else {
                                    return Err(ExecutionError::UnknownCannon(cannon.name));
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
                                    Arc::clone(&state),
                                    cannon_id,
                                    (env.id, env.storage.id, &env.aot_bin),
                                    source,
                                    sink,
                                    count,
                                )
                                .map_err(ExecutionError::Cannon)?;

                                if *awaited {
                                    let ctx = instance.ctx().unwrap();
                                    let env = Arc::clone(&env);

                                    // debug!("instance started await mode");
                                    awaiting_handles.push(tokio::task::spawn(async move {
                                        let res = ctx.spawn(rx).await;

                                        // remove the cannon after the task is complete
                                        env.cannons.remove(&cannon_id);
                                        res.map_err(ExecutionError::Cannon)
                                    }));
                                } else {
                                    instance.spawn_local(rx).map_err(ExecutionError::Cannon)?;
                                }

                                // insert the cannon
                                env.cannons.insert(cannon_id, Arc::new(instance));
                            }
                        }
                        Action::Config(configs) => {
                            for (targets, request) in configs.iter() {
                                for agent in env.matching_agents(targets, &state.pool) {
                                    // any height action will force the height to be incremented
                                    if let Some(h) = request.height {
                                        let h = h.into();
                                        set_node_field!(agent, height = (height.0 + 1, h));
                                    }

                                    // update the peers and validators
                                    if let Some(p) = &request.peers {
                                        let p: Vec<_> = env
                                            .matching_nodes(p, &state.pool, PortType::Node)
                                            .collect();
                                        set_node_field!(agent, peers = p.clone());
                                    }

                                    if let Some(p) = &request.validators {
                                        let v: Vec<_> = env
                                            .matching_nodes(p, &state.pool, PortType::Bft)
                                            .collect();
                                        set_node_field!(agent, validators = v.clone());
                                    }
                                }
                            }
                        }
                    };
                }

                // if there are any pending reconciliations,
                if !pending_reconciliations.is_empty() {
                    // reconcile all nodes
                    let task_state = Arc::clone(&state);
                    let reconcile_handle = tokio::spawn(async move {
                        if let Err(e) =
                            reconcile_agents(&task_state, pending_reconciliations.into_values())
                                .await
                        {
                            error!("failed to reconcile agents in timeline: {e}");
                            if let Err(e) =
                                Environment::forcefully_inventory(env_id, &task_state).await
                            {
                                error!("failed to inventory agents: {e}");
                            }

                            return Err(e.into());
                        };
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
                        .or_else(|| PromQuery::builtin(outcome_name))
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
