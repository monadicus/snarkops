pub mod error;
pub mod persist;
pub mod set;
pub mod timeline;

use core::fmt;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use bimap::BiMap;
use dashmap::DashMap;
use indexmap::{map::Entry, IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use snops_common::state::{
    AgentId, AgentPeer, AgentState, CannonId, EnvId, NodeKey, TimelineId, TxPipeId,
};
use tokio::{sync::Mutex, task::JoinHandle};
use tracing::{error, info, warn};

use self::{error::*, persist::PersistEnv, timeline::reconcile_agents};
use crate::{
    cannon::{
        file::{TransactionDrain, TransactionSink},
        sink::TxSink,
        source::TxSource,
        CannonInstance,
    },
    db::document::DbDocument,
    env::set::{get_agent_mappings, labels_from_nodes, pair_with_nodes, AgentMapping, BusyMode},
    error::DeserializeError,
    schema::{
        nodes::{ExternalNode, Node},
        outcomes::OutcomeMetrics,
        storage::{LoadedStorage, DEFAULT_AOT_BIN},
        timeline::TimelineEvent,
        ItemDocument, NodeTargets,
    },
    state::{Agent, GlobalState},
};

#[derive(Debug)]
pub struct Environment {
    pub id: EnvId,
    pub storage: Arc<LoadedStorage>,

    pub outcomes: OutcomeMetrics,
    // TODO: pub outcome_results: RwLock<OutcomeResults>,
    pub node_peers: BiMap<NodeKey, EnvPeer>,
    pub node_states: DashMap<NodeKey, EnvNodeState>,
    pub aot_bin: PathBuf,

    /// Map of transaction files to their respective counters
    pub tx_pipe: TxPipes,
    /// Map of cannon ids to their cannon configurations
    pub cannon_configs: DashMap<CannonId, (TxSource, TxSink)>,
    /// Map of cannon ids to their cannon instances
    pub cannons: DashMap<CannonId, Arc<CannonInstance>>,

    pub timelines: DashMap<TimelineId, Vec<TimelineEvent>>,
    pub timeline_handle: Mutex<Option<JoinHandle<Result<(), ExecutionError>>>>,
}

#[derive(Debug, Clone, Default)]
pub struct TxPipes {
    pub drains: HashMap<TxPipeId, Arc<TransactionDrain>>,
    pub sinks: HashMap<TxPipeId, Arc<TransactionSink>>,
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

        let mut storage = None;

        let (mut node_peers, mut node_states, cannons, mut tx_pipe) =
            if let Some(ref env) = prev_env {
                // stop the timeline if it's running
                // TODO: when there are multiple timelines and they run in step mode, don't do
                // this
                if let Some(handle) = &*env.timeline_handle.lock().await {
                    handle.abort();
                }

                // reuse certain elements from the previous environment with the same
                // name
                (
                    env.node_peers.clone(),
                    env.node_states.clone(),
                    // TODO: cannons instanced at prepare-time need to be removed
                    // if the instance flag is set to false or the name is changed
                    env.cannons.clone(),
                    env.tx_pipe.clone(),
                )
            } else {
                (
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                )
            };

        let cannon_configs = DashMap::default();
        let timelines = DashMap::default();
        let mut outcomes: Option<OutcomeMetrics> = None;

        let mut immediate_cannons = vec![];
        let mut agents_to_inventory = IndexSet::<AgentId>::default();

        for document in documents {
            match document {
                ItemDocument::Storage(doc) => {
                    if storage.is_none() {
                        storage = Some(doc.prepare(&state).await?);
                        // TODO: ensure storage does not change from prev_env
                    } else {
                        Err(PrepareError::MultipleStorage)?;
                    }
                }

                ItemDocument::Cannon(cannon) => {
                    cannon_configs.insert(cannon.name, (cannon.source, cannon.sink));
                    if cannon.instance {
                        immediate_cannons.push((cannon.name, cannon.count));
                    }
                }

                ItemDocument::Nodes(nodes) => {
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

                ItemDocument::Timeline(sub_timeline) => {
                    timelines.insert(sub_timeline.name, sub_timeline.timeline);
                }

                ItemDocument::Outcomes(sub_outcomes) => match outcomes {
                    Some(ref mut outcomes) => outcomes.extend(sub_outcomes.metrics.into_iter()),
                    None => outcomes = Some(sub_outcomes.metrics),
                },

                _ => warn!("ignored unimplemented document type"),
            }
        }

        let storage = storage.ok_or(PrepareError::MissingStorage)?;
        let storage_id = storage.id;
        let outcomes = outcomes.unwrap_or_default();

        // review cannon configurations to ensure all playback sources and sinks
        // have a real file backing them
        for conf in cannon_configs.iter() {
            let (source, sink) = conf.value();
            if let TxSource::Playback { file_name } = source {
                // prevent re-creating drains that were in the previous env
                if tx_pipe.drains.contains_key(file_name) {
                    continue;
                }

                tx_pipe.drains.insert(
                    *file_name,
                    Arc::new(TransactionDrain::new_unread(
                        storage.path(&state),
                        *file_name,
                    )?),
                );
            }

            if let TxSink::Record { file_name, .. } = sink {
                // prevent re-creating sinks that were in the previous env
                if tx_pipe.sinks.contains_key(file_name) {
                    continue;
                }

                tx_pipe.sinks.insert(
                    *file_name,
                    Arc::new(TransactionSink::new(storage.path(&state), *file_name)?),
                );
            }
        }

        let env = Arc::new(Environment {
            id: env_id,
            storage,
            outcomes,
            // TODO: outcome_results: Default::default(),
            node_peers,
            node_states,
            tx_pipe,
            cannon_configs,
            cannons,
            // TODO: specify the binary when uploading the test or something
            aot_bin: DEFAULT_AOT_BIN.clone(),
            timelines,
            timeline_handle: Default::default(),
        });

        if let Err(e) = PersistEnv::from(env.as_ref()).save(&state.db, env_id) {
            error!("failed to save env {env_id} to persistence: {e}");
        }

        state.envs.insert(env_id, Arc::clone(&env));

        if !agents_to_inventory.is_empty() {
            info!(
                "{env_id}: inventorying {} spare agents...",
                agents_to_inventory.len()
            );
            // reconcile agents that are freed up from the delta between environments
            if let Err(e) = reconcile_agents(
                &state,
                agents_to_inventory.into_iter().map(|id| {
                    (
                        id,
                        state.pool.get(&id).and_then(|a| a.client_owned()),
                        AgentState::Inventory,
                    )
                }),
            )
            .await
            {
                error!("an error occurred while attempting to inventory newly freed agents: {e}");
            }
        }

        // reconcile the nodes
        initial_reconcile(env_id, &state, prev_env.is_none()).await?;

        // instance cannons that are marked for immediate use
        for (name, count) in immediate_cannons {
            let Some(config) = env.cannon_configs.get(&name) else {
                continue;
            };
            let (source, sink) = config.value();

            let (mut instance, rx) = CannonInstance::new(
                Arc::clone(&state),
                name, // instanced cannons use the same name as the config
                (env_id, storage_id, &DEFAULT_AOT_BIN),
                source.clone(),
                sink.clone(),
                count,
            )?;

            // instanced cannons receive the fired count from the previous environment
            if let Some(prev_cannon) = prev_env.as_ref().and_then(|e| e.cannons.get(&name)) {
                instance.fired_txs = prev_cannon.fired_txs.clone();
            }
            instance.spawn_local(rx)?;
            env.cannons.insert(name, Arc::new(instance));
        }

        Ok(env_id)
    }

    pub async fn cleanup_timeline(
        id: EnvId,
        timeline_id: TimelineId,
        state: &GlobalState,
    ) -> Result<(), EnvError> {
        // clear the env state
        info!("clearing env {id} timeline {timeline_id} state...");

        let env = state
            .get_env(id)
            .ok_or(CleanupError::EnvNotFound(id))?
            .clone();

        env.timelines
            .remove(&timeline_id)
            .ok_or(CleanupError::TimelineNotFound(id, timeline_id))?;

        // stop the timeline if it's running
        if let Some(handle) = &*env.timeline_handle.lock().await {
            handle.abort();
        }

        Ok(())
    }

    pub async fn cleanup(id: EnvId, state: &GlobalState) -> Result<(), EnvError> {
        // clear the env state
        info!("clearing env {id} state...");

        let (_, env) = state
            .envs
            .remove(&id)
            .ok_or(CleanupError::EnvNotFound(id))?;
        if let Err(e) = PersistEnv::delete(&state.db, id) {
            error!("failed to save delete {id} to persistence: {e}");
        }

        state.prom_httpsd.lock().await.set_dirty();

        // stop the timeline if it's running
        if let Some(handle) = &*env.timeline_handle.lock().await {
            handle.abort();
        }

        if let Err(e) = reconcile_agents(
            state,
            env.node_peers
                .right_values()
                // find all agents associated with the env
                .filter_map(|peer| match peer {
                    EnvPeer::Internal(id) => Some(*id),
                    _ => None,
                })
                // this collect is necessary because the iter sent to reconcile_agents
                // must be owned by this thread. Without this, the iter would hold a reference
                // to the env.node_peers.right_values(), which is NOT Send
                .collect::<Vec<_>>()
                .into_iter()
                .map(|id| {
                    (
                        id,
                        state.pool.get(&id).and_then(|a| a.client_owned()),
                        AgentState::Inventory,
                    )
                }),
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

    pub fn matching_nodes<'a>(
        &'a self,
        targets: &'a NodeTargets,
        pool: &'a DashMap<AgentId, Agent>,
        port_type: PortType,
    ) -> impl Iterator<Item = AgentPeer> + 'a {
        self.node_peers
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

                EnvPeer::External(_key) => {
                    let entry = self.node_states.get(key)?;
                    let EnvNodeState::External(external) = entry.value() else {
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
        pool: &'a DashMap<AgentId, Agent>,
    ) -> impl Iterator<Item = dashmap::mapref::one::Ref<'a, AgentId, Agent>> {
        self.matching_nodes(targets, pool, PortType::Node) // ignore node type
            .filter_map(|agent_peer| match agent_peer {
                AgentPeer::Internal(id, _) => pool.get(&id),
                AgentPeer::External(_) => None,
            })
    }

    pub fn get_cannon(&self, id: CannonId) -> Option<Arc<CannonInstance>> {
        Some(Arc::clone(self.cannons.get(&id)?.value()))
    }
}

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

            // resolve the peers and validators
            let mut node_state = node.into_state(key.to_owned());
            node_state.private_key = node
                .key
                .as_ref()
                .map(|key| env.storage.lookup_keysource_pk(key))
                .unwrap_or_default();

            let not_me = |agent: &AgentPeer| !matches!(agent, AgentPeer::Internal(candidate_id, _) if *candidate_id == id);

            node_state.peers = env
                .matching_nodes(&node.peers, &state.pool, PortType::Node)
                .filter(not_me)
                .collect();

            node_state.validators = env
                .matching_nodes(&node.validators, &state.pool, PortType::Bft)
                .filter(not_me)
                .collect();

            let agent_state = AgentState::Node(env_id, Box::new(node_state));
            pending_reconciliations.push((id, state.get_client(id), agent_state));
        }
    }

    if let Err(e) = reconcile_agents(state, pending_reconciliations.into_iter()).await {
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
