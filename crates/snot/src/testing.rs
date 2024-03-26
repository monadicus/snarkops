use std::{
    collections::HashMap,
    fmt::Display,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use anyhow::{anyhow, bail, ensure};
use bimap::{BiHashMap, BiMap};
use futures_util::future::join_all;
use indexmap::{map::Entry, IndexMap};
use serde::Deserialize;
use snot_common::state::{AgentId, AgentPeer, AgentState, NodeKey};
use tracing::{info, warn};

use crate::{
    cannon::{sink::TxSink, source::TxSource, CannonInstance},
    schema::{
        nodes::{ExternalNode, Node},
        storage::LoadedStorage,
        ItemDocument, NodeTargets,
    },
    state::GlobalState,
};

#[derive(Debug)]
pub struct Environment {
    pub storage: Arc<LoadedStorage>,
    pub node_map: BiMap<NodeKey, EnvPeer>,
    pub initial_nodes: IndexMap<NodeKey, EnvNode>,

    /// Map of transaction files to their respective counters
    pub transaction_counters: HashMap<String, AtomicU32>,
    /// Map of cannon ids to their cannon configurations
    pub cannon_configs: HashMap<String, (TxSource, TxSink)>,
    /// Map of cannon ids to their cannon instances
    pub cannons: HashMap<String, Vec<CannonInstance>>,
}

#[derive(Debug, Clone)]
/// The effective test state of a node.
pub enum EnvNode {
    Internal(Node),
    External(ExternalNode),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
/// A way of looking up a peer in the test state.
/// Could technically use AgentPeer like this but it would have needless port
/// information
pub enum EnvPeer {
    Internal(AgentId),
    External,
}

impl Display for EnvPeer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnvPeer::Internal(id) => write!(f, "agent {id}"),
            EnvPeer::External => write!(f, "external node"),
        }
    }
}

impl Environment {
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
    /// **This will error if the current env is not unset before calling to
    /// ensure tests are properly cleaned up.**
    pub async fn prepare(
        documents: Vec<ItemDocument>,
        state: &GlobalState,
    ) -> anyhow::Result<usize> {
        let mut state_lock = state.envs.write().await;

        let mut storage = None;
        let mut node_map = BiHashMap::default();
        let mut initial_nodes = IndexMap::default();
        let mut cannon_configs = HashMap::new();
        let mut cannons = HashMap::new();

        for document in documents {
            match document {
                ItemDocument::Storage(doc) => {
                    if storage.is_none() {
                        storage = Some(doc.prepare(state).await?);
                    } else {
                        bail!("multiple storage documents found in env")
                    }
                }
                ItemDocument::Cannon(cannon) => {
                    cannon_configs.insert(cannon.name.to_owned(), (cannon.source, cannon.sink));
                    cannons.insert(cannon.name, Vec::new());
                }
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

                            match initial_nodes.entry(node_key) {
                                Entry::Occupied(ent) => bail!("duplicate node key: {}", ent.key()),
                                Entry::Vacant(ent) => {
                                    // replace the key with a new one
                                    let mut node = doc_node.to_owned();
                                    if let Some(key) = node.key.take() {
                                        node.key = Some(key.with_index(i))
                                    }
                                    ent.insert(EnvNode::Internal(node))
                                }
                            };
                        }
                    }

                    // delegate agents to become nodes
                    let pool = state.pool.read().await;
                    let available_agent = pool
                        .values()
                        .filter(|a| a.is_node_capable() && a.is_inventory());
                    let num_available_agents = available_agent.clone().count();

                    ensure!(
                        num_available_agents >= initial_nodes.len(),
                        "not enough available agents to satisfy node topology"
                    );

                    // TODO: remove this naive delegation, replace with
                    // some kind of "pick_agent" function that picks an
                    // agent best suited to be a node,
                    // instead of naively picking an agent to fill the needs of
                    // a node
                    node_map.extend(
                        initial_nodes
                            .keys()
                            .cloned()
                            .zip(available_agent.map(|agent| EnvPeer::Internal(agent.id()))),
                    );

                    info!("delegated {} nodes to agents", node_map.len());
                    for (key, node) in &node_map {
                        info!("node {key}: {node}");
                    }

                    // append external nodes to the node map

                    for (node_key, node) in &nodes.external {
                        match initial_nodes.entry(node_key.clone()) {
                            Entry::Occupied(ent) => bail!("duplicate node key: {}", ent.key()),
                            Entry::Vacant(ent) => ent.insert(EnvNode::External(node.to_owned())),
                        };
                    }
                    node_map.extend(
                        nodes
                            .external
                            .keys()
                            .cloned()
                            .map(|k| (k, EnvPeer::External)),
                    )
                }

                _ => warn!("ignored unimplemented document type"),
            }
        }

        let env = Environment {
            storage: storage.ok_or_else(|| anyhow!("env is missing storage document"))?,
            node_map,
            initial_nodes,
            transaction_counters: HashMap::new(),
            cannon_configs,
            cannons,
        };

        let env_id = state.envs_counter.fetch_add(1, Ordering::Relaxed);
        state_lock.insert(env_id, Arc::new(env));
        drop(state_lock);

        // reconcile the nodes
        initial_reconcile(&env_id, state).await?;

        Ok(env_id)
    }

    pub async fn cleanup(id: &usize, state: &GlobalState) -> anyhow::Result<()> {
        // clear the env state
        info!("clearing env {id} state...");
        let Some(env) = ({
            let mut state_lock = state.envs.write().await;
            state_lock.remove(id)
        }) else {
            bail!("env {id} not found")
        };

        // reconcile all online agents
        let (ids, handles): (Vec<_>, Vec<_>) = {
            let agents = state.pool.read().await;
            env.node_map
                .right_values()
                // find all agents associated with the env
                .filter_map(|peer| match peer {
                    EnvPeer::Internal(id) => agents.get(id),
                    _ => None,
                })
                // map the agents to rpc clients
                .filter_map(|agent| agent.client_owned().map(|client| (agent.id(), client)))
                // inventory reconcile the agents
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

    /// Lookup a env agent id by node key.
    pub fn get_agent_by_key(&self, key: &NodeKey) -> Option<AgentId> {
        self.node_map.get_by_left(key).and_then(|id| match id {
            EnvPeer::Internal(id) => Some(*id),
            EnvPeer::External => None,
        })
    }
}

/// Reconcile all associated nodes with their initial state.
pub async fn initial_reconcile(id: &usize, state: &GlobalState) -> anyhow::Result<()> {
    let mut handles = vec![];
    let mut agent_ids = vec![];
    {
        let envs_lock = state.envs.read().await;
        let env = envs_lock.get(id).ok_or_else(|| anyhow!("env not found"))?;

        let pool_lock = state.pool.read().await;

        // Lookup agent peers given a node key
        let node_to_agent = |key: &NodeKey, node: &EnvPeer, is_validator: bool| {
            // get the internal agent ID from the node key
            match node {
                // internal peers are mapped to internal agents
                EnvPeer::Internal(id) => {
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
                EnvPeer::External => {
                    let Some(EnvNode::External(external)) = env.initial_nodes.get(key) else {
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
            env.node_map
                .iter()
                .filter(|(k, _)| *k != key && target.matches(k))
                .map(|(k, v)| node_to_agent(k, v, is_validator))
                .collect()
        };

        for (key, node) in &env.initial_nodes {
            let EnvNode::Internal(node) = node else {
                continue;
            };

            // get the internal agent ID from the node key
            let Some(id) = env.get_agent_by_key(key) else {
                bail!("expected internal agent peer for node with key {key}")
            };

            let Some(client) = pool_lock.get(&id).and_then(|a| a.client_owned()) else {
                continue;
            };

            // resolve the peers and validators
            let mut node_state = node.into_state(key.ty);
            node_state.private_key = node
                .key
                .as_ref()
                .and_then(|key| env.storage.lookup_keysource(key));
            node_state.peers = matching_nodes(key, &node.peers, false)?;
            node_state.validators = matching_nodes(key, &node.validators, true)?;

            let agent_state = AgentState::Node(id, node_state);
            agent_ids.push(id);
            handles.push(tokio::spawn(async move {
                client
                    .reconcile(agent_state.clone())
                    .await
                    .map(|res| res.map(|_| agent_state))
            }));
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
            Ok(Ok(Ok(state))) => {
                agent.set_state(state);
                success += 1;
            }

            // reconcile error
            Ok(Err(e)) => warn!(
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
