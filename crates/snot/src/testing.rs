use anyhow::{anyhow, bail, ensure};
use bimap::BiMap;
use futures_util::future::join_all;
use indexmap::{map::Entry, IndexMap};
use serde::Deserialize;
use snot_common::state::{AgentId, AgentPeer, AgentState, NodeKey};
use tracing::{info, warn};

use crate::{
    schema::{
        nodes::{ExternalNode, Node},
        storage::FilenameString,
        ItemDocument, NodeTargets,
    },
    state::GlobalState,
};

#[derive(Debug, Clone)]
pub struct Test {
    pub storage_id: FilenameString,
    pub node_map: BiMap<NodeKey, TestPeer>,
    pub initial_nodes: IndexMap<NodeKey, TestNode>,
    // TODO: GlobalStorage.storage should maybe be here instead
}

#[derive(Debug, Clone)]
/// The effective test state of a node.
pub enum TestNode {
    Internal(Node),
    External(ExternalNode),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
/// A way of looking up a peer in the test state.
/// Could technically use AgentPeer like this but it would have needless port
/// information
pub enum TestPeer {
    Internal(AgentId),
    External,
}

impl Test {
    /// Deserialize (YAML) many documents into a `Vec` of documents.
    pub fn deserialize(str: &str) -> Result<Vec<ItemDocument>, anyhow::Error> {
        serde_yaml::Deserializer::from_str(str)
            .enumerate()
            .map(|(i, doc)| {
                ItemDocument::deserialize(doc).map_err(|e| anyhow!("document {i}: {e}"))
            })
            .collect()
    }

    /// Prepare a test. This will set the current test on the GlobalState.
    ///
    /// **This will error if the current test is not unset before calling to
    /// ensure tests are properly cleaned up.**
    pub async fn prepare(documents: Vec<ItemDocument>, state: &GlobalState) -> anyhow::Result<()> {
        ensure!(state.test.read().await.is_none());

        let mut state_lock = state.test.write().await;

        let Some(storage_id) = documents.iter().find_map(|s| match s {
            ItemDocument::Storage(storage) => Some(storage.id.clone()),
            _ => None,
        }) else {
            bail!("no storage document found in test")
        };

        let mut test = Test {
            storage_id,
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
                                Entry::Vacant(ent) => {
                                    // replace the key with a new one
                                    let mut node = doc_node.to_owned();
                                    if let Some(key) = node.key.take() {
                                        node.key = Some(key.replace('$', &i.to_string()))
                                    }
                                    ent.insert(TestNode::Internal(node))
                                }
                            };
                        }
                    }

                    // delegate agents to become nodes
                    let pool = state.pool.read().await;
                    let online_agents = pool.values().filter(|a| a.is_node_capable());
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
                            .zip(online_agents.map(|agent| TestPeer::Internal(agent.id()))),
                    );

                    // append external nodes to the node map

                    for (node_key, node) in &nodes.external {
                        match test.initial_nodes.entry(node_key.clone()) {
                            Entry::Occupied(ent) => bail!("duplicate node key: {}", ent.key()),
                            Entry::Vacant(ent) => ent.insert(TestNode::External(node.to_owned())),
                        };
                    }
                    test.node_map.extend(
                        nodes
                            .external
                            .keys()
                            .cloned()
                            .map(|k| (k, TestPeer::External)),
                    )
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

    // TODO: cleanup by test id, rather than cleanup EVERY agent...

    pub async fn cleanup(state: &GlobalState) -> anyhow::Result<()> {
        // clear the test state
        {
            info!("clearing test state...");
            let mut state_lock = state.test.write().await;
            *state_lock = None;
        }

        // reconcile all online agents
        let (ids, handles): (Vec<_>, Vec<_>) = {
            let agents = state.pool.read().await;
            agents
                .values()
                .filter_map(|agent| agent.client_owned().map(|client| (agent.id(), client)))
                .map(|(id, client)| {
                    (
                        id,
                        tokio::spawn(async move { client.reconcile(AgentState::Inventory).await }),
                    )
                })
                .unzip()
        };

        info!("inventorying {} agents...", ids.len());
        let reconciliations = join_all(handles).await;
        info!("reconcile done, updating agent states...");

        let mut agents = state.pool.write().await;
        let mut success = 0;
        let num_reconciles = ids.len();
        for (id, result) in ids.into_iter().zip(reconciliations) {
            match result {
                // oh god
                Ok(Ok(Ok(()))) => {
                    if let Some(agent) = agents.get_mut(&id) {
                        agent.set_state(AgentState::Inventory);
                        success += 1;
                    } else {
                        warn!("agent {id} not found in pool after successful reconcile")
                    }
                }

                // reconcile error
                Ok(Ok(Err(e))) => warn!("agent {id} experienced a reconcilation error: {e}",),

                // could be a tokio error or an RPC error
                _ => warn!("agent {id} failed to cleanup for an unknown reason"),
            }
        }
        info!("cleanup result: {success}/{num_reconciles} agents inventoried");

        Ok(())
    }
}

/// Reconcile all associated nodes with their initial state.
pub async fn initial_reconcile(state: &GlobalState) -> anyhow::Result<()> {
    let mut handles = vec![];
    let mut agent_ids = vec![];
    {
        let test_lock = state.test.read().await;
        let test = test_lock.as_ref().unwrap();

        // get the numeric storage ID from the string storage ID
        let storage_id = {
            let storage_lock = state.storage.read().await;
            match storage_lock.get_by_right(test.storage_id.as_str()) {
                Some(id) => *id,
                None => bail!("invalid storage ID specified for node"),
            }
        };

        let pool_lock = state.pool.read().await;

        // Lookup agent peers given a node key
        let node_to_agent = |key: &NodeKey, node: &TestPeer, is_validator: bool| {
            // get the internal agent ID from the node key
            match node {
                // internal peers are mapped to internal agents
                TestPeer::Internal(id) => {
                    let Some(agent) = pool_lock.get(id) else {
                        bail!("agent {id} not found in pool")
                    };

                    Ok(AgentPeer::Internal(
                        *id,
                        if is_validator {
                            agent.bft_port()
                        } else {
                            agent.node_port()
                        },
                    ))
                }
                // external peers are mapped to external nodes
                TestPeer::External => {
                    let Some(TestNode::External(external)) = test.initial_nodes.get(key) else {
                        bail!("external node with key {key} not found")
                    };

                    Ok(AgentPeer::External(if is_validator {
                        external
                            .bft
                            .ok_or_else(|| anyhow!("external node {key} is missing BFT port"))?
                    } else {
                        external
                            .node
                            .ok_or_else(|| anyhow!("external node {key} is missing Node port"))?
                    }))
                }
            }
        };

        let matching_nodes = |key: &NodeKey, target: &NodeTargets, is_validator: bool| {
            if target.is_empty() {
                return Ok(vec![]);
            }

            // this can't really be cleverly optimized into
            // a single lookup at the moment because we don't treat @local
            // as a None namespace...

            // TODO: ensure @local is always parsed as None, then we can
            // optimize each target in this to be a direct lookup
            // instead of walking through each node

            // alternatively, use a more efficient data structure for
            // storing node keys
            test.node_map
                .iter()
                .filter(|(k, _)| *k != key && target.matches(k))
                .map(|(k, v)| node_to_agent(k, v, is_validator))
                .collect()
        };

        for (key, node) in &test.initial_nodes {
            let TestNode::Internal(node) = node else {
                continue;
            };

            // get the internal agent ID from the node key
            let Some(TestPeer::Internal(id)) = test.node_map.get_by_left(key) else {
                bail!("expected internal agent peer for node with key {key}")
            };

            let Some(client) = pool_lock.get(id).and_then(|a| a.client_owned()) else {
                continue;
            };

            // resolve the peers and validators
            let mut node_state = node.into_state(key.ty);
            node_state.peers = matching_nodes(key, &node.peers, false)?;
            node_state.validators = matching_nodes(key, &node.validators, true)?;

            let agent_state = AgentState::Node(storage_id, node_state);
            agent_ids.push(*id);
            handles.push(tokio::spawn(
                async move { client.reconcile(agent_state).await },
            ));
        }
    }

    let num_attempted_reconciliations = handles.len();

    info!("waiting for reconcile...");
    let reconciliations = join_all(handles).await;
    info!("reconcile done, updating agent states...");

    let mut pool_lock = state.pool.write().await;
    let mut success = 0;
    for (agent_id, result) in agent_ids.into_iter().zip(reconciliations) {
        // safety: we acquired this before when building handles, agent_id wouldn't be
        // here if the corresponding agent didn't exist
        let agent = pool_lock.get_mut(&agent_id).unwrap();

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
