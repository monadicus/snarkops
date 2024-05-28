use snops_common::state::{AgentState, EnvId};
use tracing::error;

use super::{error::*, EnvNodeState};
use crate::{env::Environment, state::GlobalState};

/// Reconcile all associated nodes with their initial state.
pub async fn initial_reconcile(
    env_id: EnvId,
    state: &GlobalState,
    is_new_env: bool,
) -> Result<(), EnvError> {
    let mut pending_reconciliations = vec![];
    {
        let env = state
            .get_env(env_id)
            .ok_or(ReconcileError::EnvNotFound(env_id))?
            .clone();

        for entry in env.node_states.iter() {
            let key = entry.key();
            let node = entry.value();
            let EnvNodeState::Internal(node) = node else {
                continue;
            };

            // get the internal agent ID from the node key
            let id = env
                .get_agent_by_key(key)
                .ok_or_else(|| ReconcileError::ExpectedInternalAgentPeer { key: key.clone() })?;

            let node_state = env.resolve_node_state(state, id, key, node);
            let agent_state = AgentState::Node(env_id, Box::new(node_state));

            pending_reconciliations.push((id, state.get_client(id), agent_state));
        }
    }

    if let Err(e) = state.reconcile_agents(pending_reconciliations).await {
        // if this is a patch to an existing environment, avoid inventorying the agents
        if !is_new_env {
            return Err(ReconcileError::Batch(e).into());
        }

        error!("an error occurred on initial reconciliation, inventorying all agents: {e}");
        if let Err(e) = Environment::cleanup(env_id, state).await {
            error!("an error occurred inventorying agents: {e}");
        }

        Err(ReconcileError::Batch(e).into())
    } else {
        Ok(())
    }
}
