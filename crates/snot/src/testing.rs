use std::collections::HashMap;

use anyhow::{bail, ensure};
use bimap::BiMap;
use futures_util::{stream::FuturesUnordered, StreamExt};
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
}

// TODO: this is SUPER ugly (and probably really inefficient)... let's move this
// around or rewrite it later
/// Reconcile all associated nodes with their initial state.
pub async fn initial_reconcile(state: &GlobalState) -> anyhow::Result<()> {
    let test_lock = state.test.read().await;
    let pool_lock = state.pool.read().await;
    let storage_lock = state.storage.read().await;

    let test = test_lock.as_ref().unwrap();

    // the reason this needs to be kept as a new map is because we need to keep
    // ownership of `client` for the duration of `FuturesUnordered`
    let client_map = test
        .initial_nodes
        .keys()
        .filter_map(|key| {
            let agent_id = match test.node_map.get_by_left(key) {
                Some(AgentPeer::Internal(id)) => id,
                _ => return None,
            };

            let agent = pool_lock.get(&agent_id)?;
            let client = agent.client()?;

            Some((key, client))
        })
        .collect::<HashMap<_, _>>();

    let mut tasks = FuturesUnordered::new();

    for (key, client) in client_map.iter() {
        // safety: the state must exist for this node key since we derived it above
        let node = test.initial_nodes.get(*key).unwrap();

        let storage_id = match storage_lock.get_by_right(&node.storage) {
            Some(id) => *id,
            None => bail!("invalid storage ID specified for node"),
        };

        // derive states
        let node_state = node.into_state(key.ty);
        let agent_state = AgentState::Node(storage_id, node_state);

        tasks.push(client.reconcile(agent_state));
    }

    let mut success = 0;
    while let Some(r) = tasks.next().await {
        match r {
            Ok(Ok(())) => success += 1,
            Ok(Err(e)) => warn!("a node failed to reconcile: {e}"),
            Err(e) => warn!("a reconcile request for a node failed: {e}"),
        }
    }

    info!(
        "reconciliation result: {success}/{} nodes reconciled",
        client_map.len()
    );

    Ok(())
}
