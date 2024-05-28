use futures_util::future::join_all;
use snops_common::state::{AgentId, AgentState};
use tracing::{error, info};

use super::{error::BatchReconcileError, AgentClient, GlobalState};

/// The tuple to pass into `reconcile_agents`.
pub type PendingAgentReconcile = (AgentId, Option<AgentClient>, AgentState);

impl GlobalState {
    /// Reconcile a bunch of agents at once.
    pub async fn reconcile_agents(
        &self,
        iter: impl IntoIterator<Item = PendingAgentReconcile>,
    ) -> Result<(), BatchReconcileError> {
        let mut handles = vec![];
        let mut agent_ids = vec![];

        for (id, client, target) in iter {
            agent_ids.push(id);

            // if the client is present, queue a reconcile
            if let Some(client) = client {
                handles.push(tokio::spawn(async move { client.reconcile(target).await }));

                // otherwise just change the agent state so it'll inventory on
                // reconnect
            } else if let Some(mut agent) = self.pool.get_mut(&id) {
                agent.set_state(target);
                if let Err(e) = self.db.agents.save(&id, &agent) {
                    error!("failed to save agent {id} to the database: {e}");
                }
            }
        }

        if handles.is_empty() {
            return Ok(());
        }

        let num_reconciliations = handles.len();

        info!("beginning reconciliation...");
        let reconciliations = join_all(handles).await;
        info!("reconciliation complete, updating agent states...");

        let mut success = 0;
        for (agent_id, result) in agent_ids.into_iter().zip(reconciliations) {
            let Some(mut agent) = self.pool.get_mut(&agent_id) else {
                continue;
            };

            match result {
                Ok(Ok(Ok(agent_state))) => {
                    agent.set_state(agent_state);
                    if let Err(e) = self.db.agents.save(&agent_id, &agent) {
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

        self.prom_httpsd.lock().await.set_dirty();

        if success == num_reconciliations {
            Ok(())
        } else {
            Err(BatchReconcileError {
                failures: num_reconciliations - success,
            })
        }
    }
}
