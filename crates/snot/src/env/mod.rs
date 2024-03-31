pub mod set;
pub mod timeline;

use std::{
    collections::HashMap,
    fmt::Display,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use anyhow::{anyhow, bail, ensure};
use bimap::{BiHashMap, BiMap};
use futures_util::future::join_all;
use indexmap::{map::Entry, IndexMap};
use serde::Deserialize;
use snot_common::state::{AgentId, AgentPeer, AgentState, NodeKey};
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
use tracing::{info, warn};

use self::timeline::{reconcile_agents, ExecutionError};
use crate::{
    cannon::{
        file::{TransactionDrain, TransactionSink},
        sink::TxSink,
        source::TxSource,
        CannonInstance,
    },
    schema::{
        nodes::{ExternalNode, Node},
        storage::LoadedStorage,
        timeline::TimelineEvent,
        ItemDocument, NodeTargets,
    },
    state::{Agent, GlobalState},
};

#[derive(Debug)]
pub struct Environment {
    pub id: usize,
    pub storage: Arc<LoadedStorage>,
    pub node_map: BiMap<NodeKey, EnvPeer>,
    pub initial_nodes: IndexMap<NodeKey, EnvNode>,
    pub aot_bin: PathBuf,

    /// Map of transaction files to their respective counters
    pub tx_pipe: TxPipes,
    /// Map of cannon ids to their cannon configurations
    pub cannon_configs: HashMap<String, (TxSource, TxSink)>,
    /// To help generate the id of the new cannon.
    pub cannons_counter: AtomicUsize,
    /// Map of cannon ids to their cannon instances
    pub cannons: Arc<RwLock<HashMap<usize, CannonInstance>>>,

    pub timeline: Vec<TimelineEvent>,
    pub timeline_handle: Mutex<Option<JoinHandle<Result<(), ExecutionError>>>>,
}

#[derive(Debug, Clone, Default)]
pub struct TxPipes {
    pub drains: HashMap<String, Arc<TransactionDrain>>,
    pub sinks: HashMap<String, Arc<TransactionSink>>,
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

pub enum PortType {
    Node,
    Bft,
    Rest,
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
        static ENVS_COUNTER: AtomicUsize = AtomicUsize::new(0);

        let mut state_lock = state.envs.write().await;

        let mut storage = None;
        let mut node_map = BiHashMap::default();
        let mut initial_nodes = IndexMap::default();
        let mut cannon_configs = HashMap::new();
        let mut tx_pipe = TxPipes::default();
        let mut timeline = vec![];

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

                    // TODO: use node.agent and node.labels against the agent's id and labels
                    // TODO: use node.mode to determine if the agent can be a node

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

                ItemDocument::Timeline(sub_timeline) => {
                    timeline.extend(sub_timeline.timeline.into_iter());
                }

                _ => warn!("ignored unimplemented document type"),
            }
        }

        let storage = storage.ok_or_else(|| anyhow!("env is missing storage document"))?;

        // review cannon configurations to ensure all playback sources and sinks
        // have a real file backing them
        for (source, sink) in cannon_configs.values() {
            if let TxSource::Playback { file_name } = source {
                tx_pipe.drains.insert(
                    file_name.to_owned(),
                    Arc::new(TransactionDrain::new(storage.clone(), file_name)?),
                );
            }

            if let TxSink::Record { file_name, .. } = sink {
                tx_pipe.sinks.insert(
                    file_name.to_owned(),
                    Arc::new(TransactionSink::new(storage.clone(), file_name)?),
                );
            }
        }

        let env_id = ENVS_COUNTER.fetch_add(1, Ordering::Relaxed);
        let env = Environment {
            id: env_id,
            storage,
            node_map,
            initial_nodes,
            tx_pipe,
            cannon_configs,
            cannons_counter: Default::default(),
            cannons: Default::default(),
            aot_bin: {
                // TODO: specify the binary when uploading the test or something
                std::env::var("AOT_BIN")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| {
                        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("../../target/release/snarkos-aot")
                    })
            },
            timeline,
            timeline_handle: Default::default(),
        };

        state_lock.insert(env_id, Arc::new(env));
        drop(state_lock);

        // reconcile the nodes
        initial_reconcile(env_id, state).await?;

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
                Ok(Ok(Ok(state))) => {
                    if let Some(agent) = agents.get_mut(&id) {
                        agent.set_state(state);
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

    pub fn matching_nodes<'a>(
        &'a self,
        targets: &'a NodeTargets,
        pool: &'a HashMap<AgentId, Agent>,
        port_type: PortType,
    ) -> impl Iterator<Item = AgentPeer> + 'a {
        self.node_map
            .iter()
            .filter(|(key, _)| targets.matches(key))
            .filter_map(move |(key, value)| match value {
                EnvPeer::Internal(id) => {
                    let agent = pool.get(id)?;

                    Some(AgentPeer::Internal(
                        *id,
                        match port_type {
                            PortType::Bft => agent.bft_port(),
                            PortType::Node => agent.node_port(),
                            PortType::Rest => agent.rest_port(),
                        },
                    ))
                }

                EnvPeer::External => {
                    let Some(EnvNode::External(external)) = self.initial_nodes.get(key) else {
                        return None;
                    };

                    Some(AgentPeer::External(match port_type {
                        PortType::Bft => external.bft?,
                        PortType::Node => external.node?,
                        PortType::Rest => external.rest?,
                    }))
                }
            })
    }

    pub fn matching_agents<'a>(
        &'a self,
        targets: &'a NodeTargets,
        pool: &'a HashMap<AgentId, Agent>,
    ) -> impl Iterator<Item = &'a Agent> + 'a {
        self.matching_nodes(targets, pool, PortType::Node) // ignore node type
            .filter_map(|agent_peer| match agent_peer {
                AgentPeer::Internal(id, _) => pool.get(&id),
                AgentPeer::External(_) => None,
            })
    }
}

/// Reconcile all associated nodes with their initial state.
pub async fn initial_reconcile(env_id: usize, state: &GlobalState) -> anyhow::Result<()> {
    let mut pending_reconciliations = vec![];
    {
        let envs_lock = state.envs.read().await;
        let env = envs_lock
            .get(&env_id)
            .ok_or_else(|| anyhow!("env not found"))?;

        let pool_lock = state.pool.read().await;

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
                .and_then(|key| env.storage.lookup_keysource_pk(key));

            let not_me = |agent: &AgentPeer| !matches!(agent, AgentPeer::Internal(candidate_id, _) if *candidate_id == id);

            node_state.peers = env
                .matching_nodes(&node.peers, &pool_lock, PortType::Node)
                .filter(not_me)
                .collect();

            node_state.validators = env
                .matching_nodes(&node.validators, &pool_lock, PortType::Bft)
                .filter(not_me)
                .collect();

            let agent_state = AgentState::Node(env_id, node_state);
            pending_reconciliations.push((id, client, agent_state));
        }
    }

    reconcile_agents(pending_reconciliations.into_iter(), &state.pool).await?;
    Ok(())
}
