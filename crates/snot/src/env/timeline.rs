use std::sync::Arc;

use anyhow::bail;
use futures_util::future::join_all;
use thiserror::Error;
use tokio::{select, task::JoinHandle};

use super::Environment;
use crate::{
    schema::timeline::{Action, ActionInstance, EventDuration},
    state::GlobalState,
};

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("an agent is offline, so the test cannot complete")]
    AgentOffline,
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

                for ActionInstance { action, awaited } in &event.actions.0 {
                    let handle = match action {
                        // toggle online state
                        Action::Online(targets) | Action::Offline(targets) => {
                            let online = matches!(action, Action::Online(_));

                            // get target agents
                            // TODO: this is unbelievably ugly
                            let agents = env
                                .matching_agents(targets, &*pool)
                                .map(|agent| {
                                    Ok((
                                        agent.client_owned().ok_or(ExecutionError::AgentOffline)?,
                                        agent.state().clone(),
                                    ))
                                })
                                .collect::<Result<Vec<_>, _>>()?;

                            // reconcile each client agent
                            tokio::spawn(async move {
                                let handles = agents
                                    .into_iter()
                                    .map(|(client, state)| {
                                        let target_state = state.map_node(|mut n| {
                                            n.online = online;
                                            n
                                        });

                                        tokio::spawn(
                                            async move { client.reconcile(target_state).await },
                                        )
                                    })
                                    .collect::<Vec<_>>();

                                let _reconciliations = join_all(handles.into_iter()).await;

                                // TODO: update agent state in control plane
                            })
                        }

                        Action::Cannon(_) => unimplemented!(),
                        Action::Height(_) => unimplemented!(),
                    };

                    if *awaited {
                        awaiting_handles.push(handle);
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
