use core::fmt;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bimap::BiMap;
use dashmap::DashMap;
use futures_util::future::join_all;
use indexmap::{map::Entry, IndexMap, IndexSet};
use serde::Serialize;
use snops_common::{
    api::{AgentEnvInfo, EnvInfo},
    node_targets::NodeTargets,
    schema::{
        cannon::{
            sink::TxSink,
            source::{ComputeTarget, QueryTarget, TxSource},
        },
        nodes::{ExternalNode, Node},
        ItemDocument,
    },
    state::{
        AgentId, AgentPeer, AgentState, CannonId, EnvId, NetworkId, NodeKey, NodeState,
        ReconcileOptions, TxPipeId,
    },
};
use tokio::sync::Semaphore;
use tracing::{error, info, trace, warn};

use self::error::*;
use crate::{
    apply::LoadedStorage,
    cannon::{file::TransactionSink, CannonInstance, CannonInstanceMeta},
    env::set::{get_agent_mappings, labels_from_nodes, pair_with_nodes, AgentMapping, BusyMode},
    persist::PersistEnv,
    state::{Agent, GlobalState},
};

pub mod cache;
pub mod error;
pub mod set;

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
    /// Apply an environment spec. This will attempt to delegate the given node
    /// configurations to available agents, or update existing agents with new
    /// configurations.
    ///
    /// **This will error if the current env is not unset before calling to
    /// ensure tests are properly cleaned up.**
    pub async fn apply(
        env_id: EnvId,
        documents: Vec<ItemDocument>,
        state: Arc<GlobalState>,
    ) -> Result<HashMap<NodeKey, AgentId>, EnvError> {
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
                    broadcast_attempts: Some(3),
                    broadcast_timeout: TxSink::default_retry_timeout(),
                    authorize_attempts: Some(3),
                    authorize_timeout: TxSink::default_retry_timeout(),
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
                    let mut updated_states = IndexMap::<NodeKey, EnvNodeState>::default();
                    let mut incoming_peers = BiMap::default();

                    // set of resolved keys that will be present (new and old)
                    let mut agent_keys = HashSet::new();

                    for (node_key, node) in nodes.expand_internal_replicas() {
                        // Track this node as a potential agent
                        agent_keys.insert(node_key.clone());

                        // Skip delegating nodes that are already present in the node map
                        // Agents are able to determine what updates need to be applied
                        // based on their resolved node states.
                        if node_peers.contains_left(&node_key) {
                            info!("{env_id}: updating node {node_key}");
                            updated_states.insert(node_key, EnvNodeState::Internal(node));
                            continue;
                        }

                        match incoming_states.entry(node_key) {
                            Entry::Occupied(ent) => {
                                Err(PrepareError::DuplicateNodeKey(ent.key().clone()))?
                            }
                            Entry::Vacant(ent) => ent.insert(EnvNodeState::Internal(node)),
                        };
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
                    node_states.extend(updated_states.into_iter());
                }

                _ => warn!("ignored unimplemented document type"),
            }
        }

        // prepare the storage after all the other documents
        // as it depends on the network id
        let storage = LoadedStorage::from_doc(
            *storage_doc.ok_or(PrepareError::MissingStorage)?,
            &state,
            network,
        )
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

        let storage_changed = prev_env
            .as_ref()
            .is_some_and(|prev| prev.storage.info() != storage.info());

        let clear_last_height = prev_env.is_none() && !storage.persist;

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
            state
                .update_agent_states(
                    agents_to_inventory
                        .into_iter()
                        .map(|id| (id, AgentState::Inventory)),
                )
                .await;
        }

        // Emit state changes to all agents within this environment
        env.update_all_agents(
            &state,
            ReconcileOptions {
                refetch_info: storage_changed,
                clear_last_height,
                ..Default::default()
            },
        )
        .await
    }

    async fn update_all_agents(
        &self,
        state: &GlobalState,
        opts: ReconcileOptions,
    ) -> Result<HashMap<NodeKey, AgentId>, EnvError> {
        let mut pending_changes = vec![];
        let mut node_map = HashMap::new();

        for entry in self.node_states.iter() {
            let key = entry.key();
            let node = entry.value();
            let EnvNodeState::Internal(node) = node else {
                continue;
            };
            let Some(agent_id) = self.get_agent_by_key(key) else {
                continue;
            };
            let Some(agent) = state.pool.get(&agent_id) else {
                continue;
            };

            let mut next_state = self.resolve_node_state(state, agent_id, key, node);

            // determine if this reconcile will reset the agent's height (and potentially
            // trigger a ledger wipe)
            match agent.state() {
                // new environment -> reset height
                AgentState::Node(old_env, _) if *old_env != self.id => {}
                // height request is the same -> keep the height
                AgentState::Node(_, prev_state) if prev_state.height.1 == next_state.height.1 => {
                    next_state.height.0 = prev_state.height.0;
                }
                // otherwise, reset height
                AgentState::Node(_, _) => {}
                // moving from inventory -> reset height
                AgentState::Inventory => {}
            }

            node_map.insert(next_state.node_key.clone(), agent_id);

            let agent_state = AgentState::Node(self.id, Box::new(next_state));
            pending_changes.push((agent_id, agent_state));
        }

        state.update_agent_states_opts(pending_changes, opts).await;
        Ok(node_map)
    }

    pub async fn cleanup(id: EnvId, state: &GlobalState) -> Result<(), EnvError> {
        // clear the env state
        info!("{id}: Deleting persistence...");

        let env = state.remove_env(id).ok_or(CleanupError::EnvNotFound(id))?;

        if let Err(e) = state.db.envs.delete(&id) {
            error!("{id}: Failed to delete env persistence: {e}");
        }

        // TODO: write all of these values to a file before deleting them

        // cleanup cannon transaction trackers
        if let Err(e) = state.db.tx_attempts.delete_with_prefix(&id) {
            error!("{id}: Failed to delete env tx_attempts persistence: {e}");
        }
        if let Err(e) = state.db.tx_auths.delete_with_prefix(&id) {
            error!("{id}: Failed to delete env tx_auths persistence: {e}");
        }
        if let Err(e) = state.db.tx_blobs.delete_with_prefix(&id) {
            error!("{id}: Failed to delete env tx_blobs persistence: {e}");
        }
        if let Err(e) = state.db.tx_index.delete_with_prefix(&id) {
            error!("{id}: Failed to delete env tx_index persistence: {e}");
        }
        if let Err(e) = state.db.tx_status.delete_with_prefix(&id) {
            error!("{id}: Failed to delete env tx_status persistence: {e}");
        }

        if let Some(storage) = state.try_unload_storage(env.network, env.storage.id) {
            info!("{id}: Unloaded storage {}", storage.id);
        }

        trace!("{id}: Inventorying agents...");

        state
            .update_agent_states(
                env.node_peers
                    .right_values()
                    // find all agents associated with the env
                    .filter_map(|peer| match peer {
                        EnvPeer::Internal(id) => Some(*id),
                        _ => None,
                    })
                    .map(|id| (id, AgentState::Inventory))
                    // this collect is necessary because the iter sent to reconcile_agents
                    // must be owned by this thread. Without this, the iter would hold a reference
                    // to the env.node_peers.right_values(), which is NOT Send
                    .collect::<Vec<_>>(),
            )
            .await;

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

    fn nodes_with_peer<'a>(
        &'a self,
        key: &'a NodeKey,
    ) -> impl Iterator<Item = dashmap::mapref::multiple::RefMulti<'a, NodeKey, EnvNodeState>> {
        self.node_states.iter().filter(move |s| {
            // Only internal nodes can be agents
            let EnvNodeState::Internal(node) = s.value() else {
                return false;
            };

            // Ignore self-reference
            if s.key() == key {
                return false;
            }

            // Only agents that reference the node are relevant
            node.peers.matches(key) || node.validators.matches(key)
        })
    }

    pub async fn update_peer_addr(
        &self,
        state: &GlobalState,
        agent_id: AgentId,
        is_port_change: bool,
        is_ip_change: bool,
    ) {
        let Some(key) = self.get_node_key_by_agent(agent_id) else {
            return;
        };
        let pending_reconciles = self
            .nodes_with_peer(key)
            .filter_map(|ent| {
                let EnvNodeState::Internal(env_node) = ent.value() else {
                    return None;
                };

                // Lookup agent and get current state
                let agent_id = self.get_agent_by_key(ent.key())?;

                // If the port didn't change, we're not updating the agents' states
                if !is_port_change {
                    return Some((agent_id, None));
                }

                let agent = state.pool.get(&agent_id)?;

                let AgentState::Node(env_id, node_state) = agent.state() else {
                    return None;
                };

                // Determine if the node's peers and validators have changed
                let (peers, validators) = self.resolve_node_peers(&state.pool, agent_id, env_node);
                if peers == node_state.peers && validators == node_state.validators {
                    return None;
                }

                // Update the node's peers and validators
                let mut new_state = node_state.clone();
                new_state.peers = peers;
                new_state.validators = validators;

                Some((agent_id, Some(AgentState::Node(*env_id, new_state))))
            })
            .collect::<Vec<_>>();

        // Call the clear peer addr RPC for all agents that reference the node
        if is_ip_change {
            join_all(pending_reconciles.iter().filter_map(|(id, _)| {
                let client = state.get_client(*id)?;

                Some(tokio::spawn(async move {
                    client.clear_peer_addr(agent_id).await
                }))
            }))
            .await;
        }

        // Update the agent states if there's a port change
        if is_port_change {
            state
                .update_agent_states(
                    pending_reconciles
                        .into_iter()
                        .filter_map(|(id, state)| state.map(|s| (id, s))),
                )
                .await;

        // Otherwise do a normal reconcile
        } else {
            state
                .queue_many_reconciles(
                    pending_reconciles.into_iter().map(|(id, _)| id),
                    Default::default(),
                )
                .await;
        }
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

    pub fn agent_info(&self) -> AgentEnvInfo {
        AgentEnvInfo {
            network: self.network,
            storage: self.storage.info(),
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

        (node_state.peers, node_state.validators) = self.resolve_node_peers(&state.pool, id, node);

        node_state
    }

    pub fn resolve_node_peers(
        &self,
        pool: &DashMap<AgentId, Agent>,
        id: AgentId,
        node: &Node,
    ) -> (Vec<AgentPeer>, Vec<AgentPeer>) {
        // a filter to exclude the current node from the list of peers
        let not_me = |agent: &AgentPeer| !matches!(agent, AgentPeer::Internal(candidate_id, _) if *candidate_id == id);

        // resolve the peers and validators from node targets
        let mut peers: Vec<_> = self
            .matching_nodes(&node.peers, pool, PortType::Node)
            .filter(not_me)
            .collect();
        peers.sort();

        let mut validators: Vec<_> = self
            .matching_nodes(&node.validators, pool, PortType::Bft)
            .filter(not_me)
            .collect();
        validators.sort();

        (peers, validators)
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
