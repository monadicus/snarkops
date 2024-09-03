use core::fmt;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bimap::BiMap;
use dashmap::DashMap;
use indexmap::{map::Entry, IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use snops_common::{
    api::EnvInfo,
    node_targets::NodeTargets,
    state::{
        AgentId, AgentPeer, AgentState, CannonId, EnvId, NetworkId, NodeKey, NodeState, TxPipeId,
    },
};
use tokio::sync::Semaphore;
use tracing::{error, info, trace, warn};

use self::error::*;
use crate::{
    cannon::{
        file::TransactionSink,
        sink::TxSink,
        source::{ComputeTarget, QueryTarget, TxSource},
        CannonInstance, CannonInstanceMeta,
    },
    env::set::{get_agent_mappings, labels_from_nodes, pair_with_nodes, AgentMapping, BusyMode},
    error::DeserializeError,
    persist::PersistEnv,
    schema::{
        nodes::{ExternalNode, Node},
        storage::LoadedStorage,
        ItemDocument,
    },
    state::{Agent, GlobalState},
};

pub mod error;
mod reconcile;
pub mod set;
pub use reconcile::*;
pub mod cache;

#[derive(Debug)]
pub struct Environment {
    pub id: EnvId,
    pub storage: Arc<LoadedStorage>,
    pub network: NetworkId,

    // TODO: pub outcome_results: RwLock<OutcomeResults>,
    pub node_peers: BiMap<NodeKey, EnvPeer>,
    pub node_states: DashMap<NodeKey, EnvNodeState>,

    /// Map of transaction files to their respective counters
    pub sinks: HashMap<TxPipeId, Arc<TransactionSink>>,
    /// Map of cannon ids to their cannon instances
    pub cannons: HashMap<CannonId, Arc<CannonInstance>>,
}

/// The effective test state of a node.
#[derive(Debug, Clone, Serialize)]
#[allow(clippy::large_enum_variant)]
pub enum EnvNodeState {
    Internal(Node),
    External(ExternalNode),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize)]
/// A way of looking up a peer in the test state.
/// Could technically use AgentPeer like this but it would have needless port
/// information
pub enum EnvPeer {
    Internal(AgentId),
    External(NodeKey),
}

impl fmt::Display for EnvPeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvPeer::Internal(id) => write!(f, "agent {id}"),
            EnvPeer::External(k) => write!(f, "external node {k}"),
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
    pub fn deserialize(str: &str) -> Result<Vec<ItemDocument>, DeserializeError> {
        serde_yaml::Deserializer::from_str(str)
            .enumerate()
            .map(|(i, doc)| ItemDocument::deserialize(doc).map_err(|e| DeserializeError { i, e }))
            .collect()
    }

    /// Deserialize (YAML) many documents into a `Vec` of documents.
    pub fn deserialize_bytes(str: &[u8]) -> Result<Vec<ItemDocument>, DeserializeError> {
        serde_yaml::Deserializer::from_slice(str)
            .enumerate()
            .map(|(i, doc)| ItemDocument::deserialize(doc).map_err(|e| DeserializeError { i, e }))
            .collect()
    }

    /// Prepare a test. This will set the current test on the GlobalState.
    ///
    /// **This will error if the current env is not unset before calling to
    /// ensure tests are properly cleaned up.**
    pub async fn prepare(
        env_id: EnvId,
        documents: Vec<ItemDocument>,
        state: Arc<GlobalState>,
    ) -> Result<EnvId, EnvError> {
        state.prom_httpsd.lock().await.set_dirty();

        let prev_env = state.get_env(env_id);

        let mut storage_doc = None;

        let (mut node_peers, mut node_states) = if let Some(ref env) = prev_env {
            // reuse certain elements from the previous environment with the same
            // name
            (env.node_peers.clone(), env.node_states.clone())
        } else {
            (Default::default(), Default::default())
        };

        let mut network = NetworkId::default();

        let mut pending_cannons = HashMap::new();
        let mut agents_to_inventory = IndexSet::<AgentId>::default();

        // default cannon will target any node for query and broadcast target
        // any available compute will be used as well.
        pending_cannons.insert(
            CannonId::default(),
            (
                TxSource {
                    query: QueryTarget::Node(NodeTargets::ALL),
                    compute: ComputeTarget::Agent { labels: None },
                },
                TxSink {
                    target: Some(NodeTargets::ALL),
                    file_name: None,
                    broadcast_attempts: None,
                },
            ),
        );

        for document in documents {
            match document {
                ItemDocument::Storage(doc) => {
                    if storage_doc.is_none() {
                        storage_doc = Some(doc);
                        // TODO: ensure storage does not change from prev_env
                    } else {
                        Err(PrepareError::MultipleStorage)?;
                    }
                }

                ItemDocument::Cannon(cannon) => {
                    pending_cannons.insert(cannon.name, (cannon.source, cannon.sink));
                }

                ItemDocument::Nodes(nodes) => {
                    if let Some(n) = nodes.network {
                        network = n;
                    }

                    // maps of states and peers that are new to this environment
                    let mut incoming_states = IndexMap::default();
                    let mut incoming_peers = BiMap::default();

                    // set of resolved keys that will be present (new and old)
                    let mut agent_keys = HashSet::new();

                    // flatten replicas
                    for (doc_node_key, mut doc_node) in nodes.nodes {
                        let num_replicas = doc_node.replicas.unwrap_or(1);
                        // nobody needs more than 10k replicas anyway
                        for i in 0..num_replicas.min(10000) {
                            let node_key = match num_replicas {
                                0 => Err(PrepareError::NodeHas0Replicas)?,
                                1 => doc_node_key.to_owned(),
                                _ => {
                                    let mut node_key = doc_node_key.to_owned();
                                    if !node_key.id.is_empty() {
                                        node_key.id.push('-');
                                    }
                                    node_key.id.push_str(&i.to_string());
                                    node_key
                                }
                            };
                            agent_keys.insert(node_key.clone());

                            // nodes in flattened_nodes have replicas unset
                            doc_node.replicas.take();

                            // TODO: compare existing agent state with old node state
                            // where the agent state is the same, insert the new state
                            // otherwise keep the old state

                            // Skip delegating nodes that are already present in the node map
                            if node_peers.contains_left(&node_key) {
                                info!("{env_id}: skipping node {node_key} - already configured");
                                continue;
                            }

                            match incoming_states.entry(node_key) {
                                Entry::Occupied(ent) => {
                                    Err(PrepareError::DuplicateNodeKey(ent.key().clone()))?
                                }
                                Entry::Vacant(ent) => {
                                    // replace the key with a new one
                                    let mut node = doc_node.to_owned();
                                    if let Some(key) = node.key.as_mut() {
                                        *key = key.with_index(i);
                                    }
                                    ent.insert(EnvNodeState::Internal(node))
                                }
                            };
                        }
                    }

                    // list of nodes that will be removed after applying this document
                    let nodes_to_remove = node_peers
                        .iter()
                        .filter_map(|(k, v)| match v {
                            EnvPeer::Internal(_) => (!agent_keys.contains(k)).then_some(k),
                            EnvPeer::External(_) => (!nodes.external.contains_key(k)).then_some(k),
                        })
                        .cloned()
                        .collect::<Vec<_>>();

                    // get a set of all labels the nodes can reference
                    let labels = labels_from_nodes(&incoming_states);

                    for key in &nodes_to_remove {
                        info!("{env_id}: removing node {key}");
                    }

                    // list of agents that are now free because their nodes are no longer
                    // going to be part of the environment
                    let mut removed_agents = node_peers
                        .iter()
                        .filter_map(|(key, mode)| {
                            if let (EnvPeer::Internal(agent), false) =
                                (mode, agent_keys.contains(key))
                            {
                                Some(*agent)
                            } else {
                                None
                            }
                        })
                        .collect::<IndexSet<_>>();

                    // this also contains a "busy" that atomically prevents multiple
                    // environment prepares from delegating the same agents as well
                    // as preventing two nodes from claiming the same agent
                    let mut free_agents = get_agent_mappings(BusyMode::Env, &state, &labels);

                    // Additionally, include agents that are on nodes that are no longer
                    // part of this environment in the list of free agents so they can
                    // be redelegated into the same environment
                    free_agents.extend(
                        removed_agents
                            .iter()
                            .filter_map(|id| AgentMapping::from_agent_id(*id, &state, &labels)),
                    );

                    // ensure the "busy" is in scope until the initial reconcile completes and
                    // locks the agents into a non-inventory state
                    let _busy: Vec<_> =
                        match pair_with_nodes(free_agents, &incoming_states, &labels) {
                            Ok(pairs) => pairs,
                            Err(errors) => {
                                for error in &errors {
                                    error!("delegation error: {error}");
                                }
                                return Err(EnvError::Delegation(errors));
                            }
                        }
                        .map(|(key, id, busy)| {
                            // extend the node map with the newly paired agent
                            incoming_peers.insert(key, EnvPeer::Internal(id));
                            busy
                        })
                        .collect();

                    info!(
                        "{env_id}: delegated {} nodes to agents",
                        incoming_peers.len()
                    );
                    for (key, node) in &incoming_peers {
                        info!("node {key}: {node}");

                        // all re-allocated potentially removed agents are removed
                        // from the agents that will need to be inventoried
                        match node {
                            EnvPeer::Internal(agent) if removed_agents.contains(agent) => {
                                removed_agents.swap_remove(agent);
                            }
                            _ => {}
                        }
                    }

                    // all removed agents that were not recycled are pending inventory
                    agents_to_inventory.extend(removed_agents);

                    // append external nodes to the node map
                    for (node_key, node) in &nodes.external {
                        match incoming_states.entry(node_key.clone()) {
                            Entry::Occupied(ent) => {
                                Err(PrepareError::DuplicateNodeKey(ent.key().clone()))?
                            }
                            Entry::Vacant(ent) => {
                                ent.insert(EnvNodeState::External(node.to_owned()))
                            }
                        };
                    }
                    nodes.external.keys().for_each(|k| {
                        incoming_peers.insert(k.clone(), EnvPeer::External(k.clone()));
                    });

                    // remove the nodes that are no longer relevant
                    nodes_to_remove.into_iter().for_each(|key| {
                        node_peers.remove_by_left(&key);
                        node_states.remove(&key);
                    });

                    node_peers.extend(incoming_peers.into_iter());
                    node_states.extend(incoming_states.into_iter());
                }

                _ => warn!("ignored unimplemented document type"),
            }
        }

        // prepare the storage after all the other documents
        // as it depends on the network id
        let storage = storage_doc
            .ok_or(PrepareError::MissingStorage)?
            .prepare(&state, network)
            .await?;

        let storage_id = storage.id;

        // this semaphor prevents cannons from starting until the environment is
        // created
        let cannons_ready = Arc::new(Semaphore::const_new(pending_cannons.len()));
        // when this guard is dropped, the semaphore is released
        let cannons_ready_guard = Arc::clone(&cannons_ready);
        let _cannons_guard = cannons_ready_guard
            .acquire_many(pending_cannons.len() as u32)
            .await
            .unwrap();

        let compute_aot_bin = storage.resolve_compute_binary(&state).await?;

        let (cannons, sinks) = prepare_cannons(
            Arc::clone(&state),
            &storage,
            prev_env.clone(),
            cannons_ready,
            (env_id, network, storage_id, compute_aot_bin),
            pending_cannons
                .into_iter()
                .map(|(n, (source, sink))| (n, source, sink))
                .collect(),
        )?;

        let env = Arc::new(Environment {
            id: env_id,
            storage,
            network,
            node_peers,
            node_states,
            sinks,
            cannons,
        });

        if let Err(e) = state.db.envs.save(&env_id, &PersistEnv::from(env.as_ref())) {
            error!("failed to save env {env_id} to persistence: {e}");
        }

        state.insert_env(env_id, Arc::clone(&env));

        if !agents_to_inventory.is_empty() {
            info!(
                "{env_id}: inventorying {} spare agents...",
                agents_to_inventory.len()
            );
            // reconcile agents that are freed up from the delta between environments
            if let Err(e) = state
                .reconcile_agents(
                    agents_to_inventory
                        .into_iter()
                        .map(|id| (id, state.get_client(id), AgentState::Inventory)),
                )
                .await
            {
                error!("an error occurred while attempting to inventory newly freed agents: {e}");
            }
        }

        // reconcile the nodes
        initial_reconcile(env_id, &state, prev_env.is_none()).await?;

        Ok(env_id)
    }

    pub async fn cleanup(id: EnvId, state: &GlobalState) -> Result<(), EnvError> {
        // clear the env state
        info!("[env {id}] deleting persistence...");

        let env = state.remove_env(id).ok_or(CleanupError::EnvNotFound(id))?;

        if let Err(e) = state.db.envs.delete(&id) {
            error!("[env {id}] failed to delete env persistence: {e}");
        }

        // TODO: write all of these values to a file before deleting them

        // cleanup cannon transaction trackers
        if let Err(e) = state.db.tx_attempts.delete_with_prefix(&id) {
            error!("[env {id}] failed to delete env tx_attempts persistence: {e}");
        }
        if let Err(e) = state.db.tx_auths.delete_with_prefix(&id) {
            error!("[env {id}] failed to delete env tx_auths persistence: {e}");
        }
        if let Err(e) = state.db.tx_blobs.delete_with_prefix(&id) {
            error!("[env {id}] failed to delete env tx_blobs persistence: {e}");
        }
        if let Err(e) = state.db.tx_index.delete_with_prefix(&id) {
            error!("[env {id}] failed to delete env tx_index persistence: {e}");
        }
        if let Err(e) = state.db.tx_status.delete_with_prefix(&id) {
            error!("[env {id}] failed to delete env tx_status persistence: {e}");
        }

        if let Some(storage) = state.try_unload_storage(env.network, env.storage.id) {
            info!("[env {id}] unloaded storage {}", storage.id);
        }

        trace!("[env {id}] marking prom as dirty");
        state.prom_httpsd.lock().await.set_dirty();

        trace!("[env {id}] inventorying agents...");

        if let Err(e) = state
            .reconcile_agents(
                env.node_peers
                    .right_values()
                    // find all agents associated with the env
                    .filter_map(|peer| match peer {
                        EnvPeer::Internal(id) => Some(*id),
                        _ => None,
                    })
                    .map(|id| (id, state.get_client(id), AgentState::Inventory))
                    // this collect is necessary because the iter sent to reconcile_agents
                    // must be owned by this thread. Without this, the iter would hold a reference
                    // to the env.node_peers.right_values(), which is NOT Send
                    .collect::<Vec<_>>(),
            )
            .await
        {
            error!("an error occurred while attempting to inventory newly freed agents: {e}");
        }

        Ok(())
    }

    /// Lookup a env agent id by node key.
    pub fn get_agent_by_key(&self, key: &NodeKey) -> Option<AgentId> {
        self.node_peers.get_by_left(key).and_then(|id| match id {
            EnvPeer::Internal(id) => Some(*id),
            EnvPeer::External(_) => None,
        })
    }

    pub fn get_node_key_by_agent(&self, id: AgentId) -> Option<&NodeKey> {
        let peer = EnvPeer::Internal(id);
        self.node_peers.get_by_right(&peer)
    }

    pub fn matching_nodes<'a>(
        &'a self,
        targets: &'a NodeTargets,
        pool: &'a DashMap<AgentId, Agent>,
        port_type: PortType,
    ) -> impl Iterator<Item = AgentPeer> + 'a {
        self.matching_peers(targets, pool, port_type)
            .map(|(_, peer)| peer)
    }

    pub fn matching_peers<'a>(
        &'a self,
        targets: &'a NodeTargets,
        pool: &'a DashMap<AgentId, Agent>,
        port_type: PortType,
    ) -> impl Iterator<Item = (&'a NodeKey, AgentPeer)> + 'a {
        self.node_peers
            .iter()
            .filter(|(key, _)| targets.matches(key))
            .filter_map(move |(key, value)| match value {
                EnvPeer::Internal(id) => {
                    let agent = pool.get(id)?;

                    Some((
                        key,
                        AgentPeer::Internal(
                            *id,
                            match port_type {
                                PortType::Bft => agent.bft_port(),
                                PortType::Node => agent.node_port(),
                                PortType::Rest => agent.rest_port(),
                            },
                        ),
                    ))
                }

                EnvPeer::External(_key) => {
                    let entry = self.node_states.get(key)?;
                    let EnvNodeState::External(external) = entry.value() else {
                        return None;
                    };

                    Some((
                        key,
                        AgentPeer::External(match port_type {
                            PortType::Bft => external.bft?,
                            PortType::Node => external.node?,
                            PortType::Rest => external.rest?,
                        }),
                    ))
                }
            })
    }

    pub fn matching_agents<'a>(
        &'a self,
        targets: &'a NodeTargets,
        pool: &'a DashMap<AgentId, Agent>,
    ) -> impl Iterator<Item = dashmap::mapref::one::Ref<'a, AgentId, Agent>> {
        self.matching_nodes(targets, pool, PortType::Node) // ignore node type
            .filter_map(|agent_peer| match agent_peer {
                AgentPeer::Internal(id, _) => pool.get(&id),
                AgentPeer::External(_) => None,
            })
    }

    pub fn get_cannon(&self, id: CannonId) -> Option<Arc<CannonInstance>> {
        self.cannons.get(&id).cloned()
    }

    pub fn info(&self, state: &GlobalState) -> EnvInfo {
        EnvInfo {
            network: self.network,
            storage: self.storage.info(),
            block: state.get_env_block_info(self.id),
        }
    }

    /// Resolve node's agent configuration given the context of the environment.
    pub fn resolve_node_state(
        &self,
        state: &GlobalState,
        id: AgentId,
        key: &NodeKey,
        node: &Node,
    ) -> NodeState {
        // base node state
        let mut node_state = node.into_state(key.to_owned());

        // resolve the private key from the storage
        node_state.private_key = node
            .key
            .as_ref()
            .map(|key| self.storage.lookup_keysource_pk(key))
            .unwrap_or_default();

        // a filter to exclude the current node from the list of peers
        let not_me = |agent: &AgentPeer| !matches!(agent, AgentPeer::Internal(candidate_id, _) if *candidate_id == id);

        // resolve the peers and validators from node targets
        node_state.peers = self
            .matching_nodes(&node.peers, &state.pool, PortType::Node)
            .filter(not_me)
            .collect();
        node_state.peers.sort();

        node_state.validators = self
            .matching_nodes(&node.validators, &state.pool, PortType::Bft)
            .filter(not_me)
            .collect();
        node_state.validators.sort();

        node_state
    }
}

// TODO remove this type complexity problem
#[allow(clippy::type_complexity)]
pub fn prepare_cannons(
    state: Arc<GlobalState>,
    storage: &LoadedStorage,
    prev_env: Option<Arc<Environment>>,
    cannons_ready: Arc<Semaphore>,
    cannon_meta: CannonInstanceMeta,
    pending_cannons: Vec<(CannonId, TxSource, TxSink)>,
) -> Result<
    (
        HashMap<CannonId, Arc<CannonInstance>>,
        HashMap<TxPipeId, Arc<TransactionSink>>,
    ),
    EnvError,
> {
    let mut cannons = HashMap::default();
    let mut sinks = HashMap::default();

    for (name, source, sink) in pending_cannons.into_iter() {
        // create file sinks for all the cannons that use files as output
        if let Some(file_name) = sink.file_name {
            // prevent re-creating sinks that were in the previous env
            if let std::collections::hash_map::Entry::Vacant(e) = sinks.entry(file_name) {
                e.insert(Arc::new(TransactionSink::new(
                    storage.path(&state),
                    file_name,
                )?));
            }
        }

        let (mut instance, rx) = CannonInstance::new(
            Arc::clone(&state),
            name, // instanced cannons use the same name as the config
            cannon_meta.clone(),
            source,
            sink,
        )?;

        // instanced cannons receive the fired count from the previous environment
        if let Some(prev_cannon) = prev_env.as_ref().and_then(|e| e.cannons.get(&name)) {
            instance.fired_txs = prev_cannon.fired_txs.clone();
        }
        instance.spawn_local(rx, Arc::clone(&cannons_ready))?;
        cannons.insert(name, Arc::new(instance));
    }

    Ok((cannons, sinks))
}
