use std::{
    collections::{HashMap, HashSet},
    sync::{mpsc, Arc, Weak},
};

use fixedbitset::FixedBitSet;
use indexmap::IndexMap;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use snops_common::{
    lasso::Spur,
    set::MASK_PREFIX_LEN,
    state::{AgentId, NodeKey},
};

use super::{DelegationError, EnvNodeState};
use crate::state::{Agent, AgentClient, Busy, GlobalState};

pub struct AgentMapping {
    id: AgentId,
    claim: Weak<Busy>,
    mask: FixedBitSet,
}

/// Ways of describing how an agent can be busy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyMode {
    /// The agent is busy with a compute task
    Compute,
    /// The agent is busy with an env task
    Env,
}

impl AgentMapping {
    pub fn new(mode: BusyMode, agent: &Agent, labels: &[Spur]) -> Option<Self> {
        if !agent.is_inventory() {
            return None;
        }

        // check if the agent is available in the given mode
        let claim = match mode {
            BusyMode::Compute => {
                if !agent.can_compute() {
                    return None;
                }
                agent.get_compute_claim()
            }
            BusyMode::Env => {
                if !(agent.is_node_capable() && agent.is_inventory()) {
                    return None;
                }
                agent.get_env_claim()
            }
        };

        // check if the agent is already claimed
        if claim.strong_count() > 1 {
            return None;
        }

        Some(Self {
            id: agent.id(),
            claim,
            mask: agent.mask(labels),
        })
    }

    pub fn from_agent_id(agent_id: AgentId, state: &GlobalState, labels: &[Spur]) -> Option<Self> {
        state.pool.get(&agent_id).map(|agent| Self {
            id: agent_id,
            claim: agent.get_env_claim(),
            mask: agent.mask(labels),
        })
    }

    /// Attempt to atomically claim the agent
    pub fn claim(&self) -> Option<Arc<Busy>> {
        // avoid needlessly upgrading the weak pointer
        if self.claim.strong_count() > 1 {
            return None;
        }

        let arc = self.claim.upgrade()?;
        // 2 because the agent owns arc, and this would be the second
        // there is a slim chance that two nodes could claim the same agent. if we run
        // into this we can add an AtomicBool to the mapping to determine if the
        // agent is claimed by the node on this thread
        (Arc::strong_count(&arc) == 2).then_some(arc)
    }

    /// Attempt to atomically claim the agent if there is a mask subset
    pub fn claim_if_subset(&self, mask: &FixedBitSet) -> Option<Arc<Busy>> {
        if mask.is_subset(&self.mask) {
            self.claim()
        } else {
            None
        }
    }
}

/// Convert an iterator of agents into a vec of agent mappings
/// This is necessary the so the pool of agents can be dropped for longer
/// running tasks
pub fn get_agent_mappings(
    mode: BusyMode,
    state: &GlobalState,
    labels: &[Spur],
) -> Vec<AgentMapping> {
    state
        .pool
        .iter()
        .filter_map(|agent| AgentMapping::new(mode, &agent, labels))
        .collect()
}

/// Get a list of unique labels given a node config
pub fn labels_from_nodes(nodes: &IndexMap<NodeKey, EnvNodeState>) -> Vec<Spur> {
    let mut labels = HashSet::new();

    for node in nodes.values() {
        match node {
            EnvNodeState::Internal(n) => {
                labels.extend(&n.labels);
            }
            EnvNodeState::External(_) => {}
        }
    }

    labels.into_iter().collect()
}

/// Find an agent that can compute and has the given labels using a fixedbitset
/// (SIMD)
///
/// This approach would make more sense if we had a variety of masks (sets of
/// labels) Rather than checking against a finite mask.
fn _find_compute_agent_by_mask<'a, I: Iterator<Item = &'a Agent>>(
    mut agents: I,
    labels: &[Spur],
) -> Option<(&'a Agent, Arc<Busy>)> {
    // replace with
    let mut mask = FixedBitSet::with_capacity(labels.len() + MASK_PREFIX_LEN);
    mask.insert_range(MASK_PREFIX_LEN..labels.len() + MASK_PREFIX_LEN);

    agents.find_map(|agent| {
        AgentMapping::new(BusyMode::Compute, agent, labels)
            .and_then(|m| m.claim_if_subset(&mask).map(|arc| (agent, arc)))
    })
}

/// Find an agent that can compute and has the given labels by checking each
/// label individually
pub fn find_compute_agent(
    state: &GlobalState,
    labels: &[Spur],
) -> Option<(AgentId, AgentClient, Arc<Busy>)> {
    state.pool.iter().find_map(|a| {
        if !a.can_compute() || a.is_compute_claimed() || !labels.iter().all(|l| a.has_label(*l)) {
            return None;
        }
        let arc = a.make_busy();
        a.client_owned()
            .and_then(|c| (Arc::strong_count(&arc) == 2).then_some((a.id(), c, arc)))
    })
}

/// Given a map of nodes and list of agent mappings, attempt to pair each node
/// with an agent in parallel
pub fn pair_with_nodes(
    agents: Vec<AgentMapping>,
    nodes: &IndexMap<NodeKey, EnvNodeState>,
    labels: &[Spur],
) -> Result<impl Iterator<Item = (NodeKey, AgentId, Arc<Busy>)>, Vec<DelegationError>> {
    // errors that occurred while pairing nodes with agents
    let (errors_tx, errors_rx) = mpsc::channel();
    // nodes that were successfully claimed. dropping this will automatically
    // unclaim the agents
    let (claimed_tx, claimed_rx) = mpsc::channel();

    let (want_ids, want_labels) = nodes
        .iter()
        // filter out external nodes
        // split into nodes that want specific agents and nodes that want specific labels
        .filter_map(|(key, env_node)| match env_node {
            EnvNodeState::Internal(n) => match n.agent {
                Some(agent) => Some((Some((key, agent)), None)),
                None => Some((None, Some((key, n.mask(key, labels))))),
            },
            EnvNodeState::External(_) => None,
        })
        // unzip and filter out the Nones
        .fold((vec![], vec![]), |(mut vec_a, mut vec_b), (a, b)| {
            if let Some(a) = a {
                vec_a.push(a);
            }
            if let Some(b) = b {
                vec_b.push(b);
            }
            (vec_a, vec_b)
        });

    let num_needed = want_ids.len() + want_labels.len();
    let num_available = agents.len();
    if num_available < num_needed {
        return Err(vec![DelegationError::InsufficientAgentCount(
            num_available,
            num_needed,
        )]);
    }

    // handle the nodes that want specific agents first
    let agent_map = agents.iter().map(|a| (a.id, a)).collect::<HashMap<_, _>>();

    // walk through all the nodes that want specific agents and attempt to pair them
    // with an agent
    want_ids.into_par_iter().for_each(|(key, id)| {
        // ensure the agent exists
        let Some(agent) = agent_map.get(&id) else {
            let _ = errors_tx.send(DelegationError::AgentNotFound(id, key.clone()));
            return;
        };

        // ensure this agent supports the needed mode
        if !agent.mask.contains(key.ty.bit()) {
            let _ = errors_tx.send(DelegationError::AgentMissingMode(id, key.clone()));
            return;
        }

        // attempt to claim the agent
        if let Some(claim) = agent.claim() {
            let _ = claimed_tx.send((key.clone(), id, claim));
        } else {
            let _ = errors_tx.send(DelegationError::AgentAlreadyClaimed(id, key.clone()));
        }
    });

    // walk through all the nodes that want specific labels/modes and attempt to
    // pair them with an agent that has the matching mask
    want_labels.into_par_iter().for_each(|(key, mask)| {
        // find the first agent that can be claimed that fits the mask
        if let Some((id, claim)) = agents
            .iter()
            .find_map(|a| a.claim_if_subset(&mask).map(|c| (a.id, c)))
        {
            let _ = claimed_tx.send((key.clone(), id, claim));
        } else {
            let _ = errors_tx.send(DelegationError::NoAvailableAgents(key.clone()));
        }
    });

    let errors = errors_rx.try_iter().collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(claimed_rx.into_iter())
    } else {
        Err(errors)
    }
}
