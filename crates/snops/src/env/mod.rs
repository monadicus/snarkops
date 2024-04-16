pub mod error;
pub mod persist;
pub mod set;
pub mod timeline;

use core::fmt;
use std::{collections::HashMap, path::PathBuf, sync::Arc};

use bimap::{BiHashMap, BiMap};
use futures_util::future::join_all;
use indexmap::{map::Entry, IndexMap};
use serde::{Deserialize, Serialize};
use snops_common::state::{
    AgentId, AgentPeer, AgentState, CannonId, EnvId, NodeKey, TimelineId, TxPipeId,
};
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
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
    env::set::{get_agent_mappings, labels_from_nodes, pair_with_nodes, BusyMode},
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
    pub node_map: BiMap<NodeKey, EnvPeer>,
    pub initial_nodes: IndexMap<NodeKey, EnvNode>,
    pub aot_bin: PathBuf,

    /// Map of transaction files to their respective counters
    pub tx_pipe: TxPipes,
    /// Map of cannon ids to their cannon configurations
    pub cannon_configs: HashMap<CannonId, (TxSource, TxSink)>,
    /// Map of cannon ids to their cannon instances
    pub cannons: Arc<RwLock<HashMap<CannonId, CannonInstance>>>,

    pub timelines: HashMap<TimelineId, Vec<TimelineEvent>>,
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
pub enum EnvNode {
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
        let mut state_lock = state.envs.write().await;
        state.prom_httpsd.lock().await.set_dirty();

        let mut storage = None;
        let mut node_map = BiHashMap::default();
        let mut initial_nodes = IndexMap::default();
        let mut cannon_configs = HashMap::new();
        let mut tx_pipe = TxPipes::default();
        let mut timelines = HashMap::new();
        let mut outcomes: Option<OutcomeMetrics> = None;

        let mut immediate_cannons = vec![];

        for document in documents {
            match document {
                ItemDocument::Storage(doc) => {
                    if storage.is_none() {
                        storage = Some(doc.prepare(&state).await?);
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
                    // flatten replicas
                    for (doc_node_key, mut doc_node) in nodes.nodes {
                        let num_replicas = doc_node.replicas.unwrap_or(1);
                        for i in 0..num_replicas {
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

                            // nodes in flattened_nodes have replicas unset
                            doc_node.replicas.take();

                            match initial_nodes.entry(node_key) {
                                Entry::Occupied(ent) => {
                                    Err(PrepareError::DuplicateNodeKey(ent.key().clone()))?
                                }
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

                    // get a set of all labels the nodes can reference
                    let labels = labels_from_nodes(&initial_nodes);

                    // temporarily lock the agent pool for reading to convert them into
                    // masks against the labels.
                    //
                    // this also contains a "busy" that atomically prevents multiple
                    // environment prepares from delegating the same agents as well
                    // as preventing two nodes from claiming the same agent
                    let agents = get_agent_mappings(
                        BusyMode::Env,
                        state.pool.read().await.values(),
                        &labels,
                    );

                    // ensure the "busy" is in scope until the initial reconcile completes and
                    // locks the agents into a non-inventory state
                    let _busy: Vec<_> = match pair_with_nodes(agents, &initial_nodes, &labels) {
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
                        node_map.insert(key, EnvPeer::Internal(id));
                        busy
                    })
                    .collect();

                    info!("delegated {} nodes to agents", node_map.len());
                    for (key, node) in &node_map {
                        info!("node {key}: {node}");
                    }

                    // append external nodes to the node map

                    for (node_key, node) in &nodes.external {
                        match initial_nodes.entry(node_key.clone()) {
                            Entry::Occupied(ent) => {
                                Err(PrepareError::DuplicateNodeKey(ent.key().clone()))?
                            }
                            Entry::Vacant(ent) => ent.insert(EnvNode::External(node.to_owned())),
                        };
                    }
                    nodes.external.keys().for_each(|k| {
                        node_map.insert(k.clone(), EnvPeer::External(k.clone()));
                    })
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
        let outcomes = outcomes.unwrap_or_default();

        // review cannon configurations to ensure all playback sources and sinks
        // have a real file backing them
        for (source, sink) in cannon_configs.values() {
            if let TxSource::Playback { file_name } = source {
                tx_pipe.drains.insert(
                    *file_name,
                    Arc::new(TransactionDrain::new_unread(
                        storage.path(&state),
                        *file_name,
                    )?),
                );
            }

            if let TxSink::Record { file_name, .. } = sink {
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
            node_map,
            initial_nodes,
            tx_pipe,
            cannon_configs,
            cannons: Default::default(),
            // TODO: specify the binary when uploading the test or something
            aot_bin: DEFAULT_AOT_BIN.clone(),
            timelines,
            timeline_handle: Default::default(),
        });

        state_lock.insert(env_id, Arc::clone(&env));
        if let Err(e) = PersistEnv::from(env.as_ref()).save(&state.db, env_id) {
            error!("failed to save env {env_id} to persistence: {e}");
        }
        drop(state_lock);

        // reconcile the nodes
        initial_reconcile(env_id, &state).await?;

        // instance cannons that are marked for immediate use
        let mut cannons = env.cannons.write().await;
        for (name, count) in immediate_cannons {
            let Some((source, sink)) = env.cannon_configs.get(&name) else {
                continue;
            };

            let (mut instance, rx) = CannonInstance::new(
                Arc::clone(&state),
                name, // instanced cannons use the same name as the config
                Arc::clone(&env),
                source.clone(),
                sink.clone(),
                count,
            )?;
            instance.spawn_local(rx)?;
            cannons.insert(name, instance);
        }

        Ok(env_id)
    }

    pub async fn cleanup_timeline(
        id: &EnvId,
        timeline_id: &TimelineId,
        state: &GlobalState,
    ) -> Result<(), EnvError> {
        // clear the env state
        info!("clearing env {id} timeline {timeline_id} state...");

        let mut lock = state.envs.write().await;
        let env = Arc::get_mut(lock.get_mut(id).ok_or(CleanupError::EnvNotFound(*id))?).unwrap();

        env.timelines
            .remove(timeline_id)
            .ok_or(CleanupError::TimelineNotFound(*id, *timeline_id))?;

        // we could just call cleanup for now lol
        unimplemented!(
            "we need to reconcile the agents associated with the timeline after removing"
        );
    }

    pub async fn cleanup(id: &EnvId, state: &GlobalState) -> Result<(), EnvError> {
        // clear the env state
        info!("clearing env {id} state...");

        // TODO do more with timeline_id here
        let env = state
            .envs
            .write()
            .await
            .remove(id)
            .ok_or(CleanupError::EnvNotFound(*id))?;
        if let Err(e) = PersistEnv::delete(&state.db, *id) {
            error!("failed to save delete {id} to persistence: {e}");
        }

        state.prom_httpsd.lock().await.set_dirty();

        // stop the timeline if it's running
        if let Some(handle) = &*env.timeline_handle.lock().await {
            handle.abort();
        }

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
                Ok(Ok(Ok(agent_state))) => {
                    if let Some(agent) = agents.get_mut(&id) {
                        agent.set_state(agent_state);
                        if let Err(e) = agent.save(&state.db, id) {
                            error!("failed to save agent {id} to the database: {e}");
                        }
                        success += 1;
                    } else {
                        error!("agent {id} not found in pool after successful reconcile")
                    }
                }

                // reconcile error
                Ok(Ok(Err(e))) => error!("agent {id} experienced a reconcilation error: {e}"),
                Ok(Err(e)) => error!("agent {id} experienced a rpc error: {e}"),
                Err(e) => error!("agent {id} experienced a join error: {e}"),
            }
        }
        info!("cleanup result: {success}/{num_reconciles} agents inventoried");

        Ok(())
    }

    // TODO: this is almost exactly the same as `cleanup`, maybe we can merge it
    // later
    pub async fn forcefully_inventory(id: EnvId, state: &GlobalState) -> Result<(), EnvError> {
        let mut envs_lock = state.envs.write().await;
        let env = envs_lock
            .get_mut(&id)
            .ok_or(CleanupError::EnvNotFound(id))?;

        // stop the timeline if it's running
        if let Some(handle) = &*env.timeline_handle.lock().await {
            handle.abort();
        }

        // reconcile all online agents
        let (ids, handles): (Vec<_>, Vec<_>) = {
            let mut agents = state.pool.write().await;

            let mut ids = vec![];
            let mut handles = vec![];
            for peer in env.node_map.right_values() {
                let Some(agent) = (match peer {
                    EnvPeer::Internal(id) => agents.get_mut(id),
                    _ => continue,
                }) else {
                    continue;
                };

                let Some(client) = agent.client_owned() else {
                    // forcibly set the agent state if it is offline
                    agent.set_state(AgentState::Inventory);
                    if let Err(e) = agent.save(&state.db, id) {
                        error!("failed to save agent {id} to the database: {e}");
                    }
                    continue;
                };

                ids.push(agent.id());
                handles.push(tokio::spawn(async move {
                    client.reconcile(AgentState::Inventory).await
                }));
            }

            (ids, handles)
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
                Ok(Ok(Ok(agent_state))) => {
                    if let Some(agent) = agents.get_mut(&id) {
                        agent.set_state(agent_state);
                        if let Err(e) = agent.save(&state.db, id) {
                            error!("failed to save agent {id} to the database: {e}");
                        }
                        success += 1;
                    } else {
                        error!("agent {id} not found in pool after successful reconcile")
                    }
                }

                // reconcile error
                Ok(Ok(Err(e))) => error!("agent {id} experienced a reconcilation error: {e}"),
                Ok(Err(e)) => error!("agent {id} experienced a rpc error: {e}"),
                Err(e) => error!("agent {id} experienced a join error: {e}"),
            }
        }
        info!("inventory result: {success}/{num_reconciles} agents inventoried");

        Ok(())
    }

    /// Lookup a env agent id by node key.
    pub fn get_agent_by_key(&self, key: &NodeKey) -> Option<AgentId> {
        self.node_map.get_by_left(key).and_then(|id| match id {
            EnvPeer::Internal(id) => Some(*id),
            EnvPeer::External(_) => None,
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

                EnvPeer::External(_key) => {
                    let Some(EnvNode::External(external)) = self.initial_nodes.get(key) else {
                        info!("ignoring node {key}");
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
pub async fn initial_reconcile(env_id: EnvId, state: &GlobalState) -> Result<(), EnvError> {
    let mut pending_reconciliations = vec![];
    {
        let envs_lock = state.envs.read().await;
        let env = envs_lock
            .get(&env_id)
            .ok_or(ReconcileError::EnvNotFound(env_id))?;

        let pool_lock = state.pool.read().await;

        for (key, node) in &env.initial_nodes {
            let EnvNode::Internal(node) = node else {
                continue;
            };

            // get the internal agent ID from the node key
            let id = env
                .get_agent_by_key(key)
                .ok_or_else(|| ReconcileError::ExpectedInternalAgentPeer { key: key.clone() })?;

            let Some(client) = pool_lock.get(&id).and_then(|a| a.client_owned()) else {
                continue;
            };

            // resolve the peers and validators
            let mut node_state = node.into_state(key.to_owned());
            node_state.private_key = node
                .key
                .as_ref()
                .map(|key| env.storage.lookup_keysource_pk(key))
                .unwrap_or_default();

            let not_me = |agent: &AgentPeer| !matches!(agent, AgentPeer::Internal(candidate_id, _) if *candidate_id == id);

            node_state.peers = env
                .matching_nodes(&node.peers, &pool_lock, PortType::Node)
                .filter(not_me)
                .collect();

            node_state.validators = env
                .matching_nodes(&node.validators, &pool_lock, PortType::Bft)
                .filter(not_me)
                .collect();

            let agent_state = AgentState::Node(env_id, Box::new(node_state));
            pending_reconciliations.push((id, client, agent_state));
        }
    }

    if let Err(e) = reconcile_agents(state, pending_reconciliations.into_iter(), &state.pool).await
    {
        error!("an error occurred on initial reconciliation, inventorying all agents: {e}");
        if let Err(e) = Environment::forcefully_inventory(env_id, state).await {
            error!("an error occurred inventorying agents: {e}");
        }

        Err(ReconcileError::Batch(e).into())
    } else {
        Ok(())
    }
}
