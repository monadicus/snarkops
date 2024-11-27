use std::collections::HashMap;

use futures_util::future::join_all;
use snops_common::state::{AgentId, AgentState, NodeKey};
use tracing::{error, info};

use super::GlobalState;

/// The tuple to pass into `reconcile_agents`.
pub type PendingAgentReconcile = (AgentId, AgentState);

/// Get a node map (key => agent ID) from an agent reconciliation iterator.
pub fn pending_reconcile_node_map<'a>(
    pending: impl Iterator<Item = &'a PendingAgentReconcile>,
) -> HashMap<NodeKey, AgentId> {
    pending
        .map(|(id, state)| match state {
            AgentState::Node(_, node) => (node.node_key.clone(), *id),
            _ => unreachable!(),
        })
        .collect::<HashMap<_, _>>()
}

impl GlobalState {
    /// Reconcile a bunch of agents at once.
    pub async fn update_agent_states(&self, iter: impl IntoIterator<Item = PendingAgentReconcile>) {
        let mut agent_ids = vec![];

        for (id, target) in iter {
            if let Some(mut agent) = self.pool.get_mut(&id) {
                agent_ids.push(id);
                agent.set_state(target);
                if let Err(e) = self.db.agents.save(&id, &agent) {
                    error!("failed to save agent {id} to the database: {e}");
                }
            }
        }

        self.queue_many_reconciles(agent_ids).await;
    }

    pub async fn queue_many_reconciles(
        &self,
        iter: impl IntoIterator<Item = AgentId>,
    ) -> (usize, usize) {
        let mut handles = vec![];
        let mut agent_ids = vec![];

        for id in iter {
            let agent = self.pool.get(&id);
            let Some(agent) = agent else {
                continue;
            };
            let Some(client) = agent.client_owned() else {
                continue;
            };

            agent_ids.push(id);
            let target = agent.state.clone();

            handles.push(tokio::spawn(
                async move { client.set_agent_state(target).await },
            ));
        }

        if handles.is_empty() {
            return (0, 0);
        }

        let num_reconciliations = handles.len();

        info!("Queuing reconciliation...");
        let reconciliations = join_all(handles).await;

        let mut success = 0;
        for (agent_id, result) in agent_ids.into_iter().zip(reconciliations) {
            match result {
                Ok(Ok(())) => {
                    success += 1;
                }
                Ok(Err(e)) => error!("agent {agent_id} experienced a rpc error: {e}"),
                Err(e) => error!("join error during agent {agent_id} reconcile: {e}"),
            }
        }

        info!(
            "reconciliation result: {success}/{} nodes connected",
            num_reconciliations
        );

        (success, num_reconciliations)
    }
}
