use anyhow::{bail, ensure};
use bimap::BiMap;
use futures_util::future::join_all;
use indexmap::{map::Entry, IndexMap};
use serde::Deserialize;
use snot_common::state::{AgentPeer, AgentState, NodeKey};
use tracing::{info, warn};

use crate::{
    schema::{nodes::Node, ItemDocument},
    state::GlobalState,
};

#[derive(Debug, Clone)]
pub struct Test {
    pub node_map: BiMap<NodeKey, AgentPeer>,
    pub initial_nodes: IndexMap<NodeKey, Node>,
    // TODO: GlobalStorage.storage should maybe be here instead
}

impl Test {
    /// Deserialize (YAML) many documents into a `Vec` of documents.
    pub fn deserialize(str: &str) -> Result<Vec<ItemDocument>, serde_yaml::Error> {
        serde_yaml::Deserializer::from_str(str)
            .map(ItemDocument::deserialize)
            .collect()
    }

    /// Prepare a test. This will set the current test on the GlobalState.
    ///
    /// **This will error if the current test is not unset before calling to
    /// ensure tests are properly cleaned up.**
    pub async fn prepare(documents: Vec<ItemDocument>, state: &GlobalState) -> anyhow::Result<()> {
        ensure!(state.test.read().await.is_none());

        let mut state_lock = state.test.write().await;

        let mut test = Test {
            node_map: Default::default(),
            initial_nodes: Default::default(),
        };

        for document in documents {
            match document {
                ItemDocument::Storage(storage) => storage.prepare(state).await?,
                ItemDocument::Nodes(nodes) => {
                    // flatten replicas
                    for (doc_node_key, mut doc_node) in nodes.nodes {
                        let num_replicas = doc_node.replicas.unwrap_or(1);
                        for i in 0..num_replicas {
                            let node_key = match num_replicas {
                                0 => bail!("cannot have a node with zero replicas"),
                                1 => doc_node_key.to_owned(),
                                _ => {
                                    let mut node_key = doc_node_key.to_owned();
                                    node_key.id.push('-');
                                    node_key.id.push_str(&i.to_string());
                                    node_key
                                }
                            };

                            // nodes in flattened_nodes have replicas unset
                            doc_node.replicas.take();

                            match test.initial_nodes.entry(node_key) {
                                Entry::Occupied(ent) => bail!("duplicate node key: {}", ent.key()),
                                Entry::Vacant(ent) => ent.insert(doc_node.to_owned()),
                            };
                        }
                    }

                    // TODO: external nodes
                    // for (node_key, node) in nodes.external {
                    // }

                    // delegate agents to become nodes
                    let pool = state.pool.read().await;
                    let online_agents = pool.values().filter(|a| a.is_connected());
                    let num_online_agents = online_agents.clone().count();

                    ensure!(
                        num_online_agents >= test.initial_nodes.len(),
                        "not enough online agents to satisfy node topology"
                    );

                    // TODO: remove this naive delegation, replace with
                    // some kind of "pick_agent" function that picks an
                    // agent best suited to be a node,
                    // instead of naively picking an agent to fill the needs of
                    // a node
                    test.node_map.extend(
                        test.initial_nodes
                            .keys()
                            .cloned()
                            .zip(online_agents.map(|agent| AgentPeer::Internal(agent.id()))),
                    );
                }

                _ => warn!("ignored unimplemented document type"),
            }
        }

        // set the test on the global state
        *state_lock = Some(test);
        drop(state_lock);

        // reconcile the nodes
        initial_reconcile(state).await?;

        Ok(())
    }

    pub async fn cleanup(state: &GlobalState) -> anyhow::Result<()> {
        let mut state_lock = state.test.write().await;
        let mut agents = state.pool.write().await;

        *state_lock = None;

        // reconcile all online agents
        let handles = agents
            .values()
            .filter_map(|agent| agent.client_owned())
            .map(|client| {
                tokio::spawn(async move { client.reconcile(AgentState::Inventory).await })
            });

        let reconciliations = join_all(handles).await;

        for (agent, result) in agents.values_mut().zip(reconciliations) {
            match result {
                // oh god
                Ok(Ok(Ok(()))) => agent.set_state(AgentState::Inventory),

                // reconcile error
                Ok(Ok(Err(e))) => warn!(
                    "agent {} experienced a reconcilation error: {e}",
                    agent.id()
                ),

                // could be a tokio error or an RPC error
                _ => warn!(
                    "agent {} failed to cleanup for an unknown reason",
                    agent.id()
                ),
            }
        }

        Ok(())
    }
}

/// Reconcile all associated nodes with their initial state.
pub async fn initial_reconcile(state: &GlobalState) -> anyhow::Result<()> {
    let test_lock = state.test.read().await;
    let mut pool_lock = state.pool.write().await;
    let storage_lock = state.storage.read().await;

    let test = test_lock.as_ref().unwrap();

    let mut handles = vec![];
    let mut agent_ids = vec![];
    for (key, node) in &test.initial_nodes {
        // get the numeric storage ID from the string storage ID
        let storage_id = match storage_lock.get_by_right(&node.storage) {
            Some(id) => *id,
            None => bail!("invalid storage ID specified for node"),
        };

        // get the internal agent ID from the node key
        let Some(AgentPeer::Internal(id)) = test.node_map.get_by_left(key) else {
            continue;
        };

        let Some(agent) = pool_lock.get(&id) else {
            continue;
        };

        let Some(client) = agent.client_owned() else {
            continue;
        };

        let agent_state = AgentState::Node(storage_id, node.into_state(key.ty));
        agent_ids.push(id);
        handles.push(tokio::spawn(
            async move { client.reconcile(agent_state).await },
        ));
    }

    let num_attempted_reconciliations = handles.len();
    let reconciliations = join_all(handles).await;

    let mut success = 0;
    for (agent_id, result) in agent_ids.into_iter().zip(reconciliations) {
        // safety: we acquired this before when building handles, agent_id wouldn't be
        // here if the corresponding agent didn't exist
        let agent = pool_lock.get_mut(agent_id).unwrap();

        match result {
            // oh god
            Ok(Ok(Ok(()))) => {
                agent.set_state(AgentState::Inventory);
                success += 1;
            }

            // reconcile error
            Ok(Ok(Err(e))) => warn!(
                "agent {} experienced a reconcilation error: {e}",
                agent.id()
            ),

            // could be a tokio error or an RPC error
            _ => warn!(
                "agent {} failed to reconcile for an unknown reason",
                agent.id()
            ),
        }
    }

    info!(
        "reconciliation result: {success}/{} nodes reconciled",
        num_attempted_reconciliations
    );

    Ok(())
}
