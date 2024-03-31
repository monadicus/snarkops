use std::{
    collections::{HashMap, HashSet},
    sync::{mpsc, Arc, Weak},
};

use fixedbitset::FixedBitSet;
use indexmap::IndexMap;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use snot_common::{
    lasso::Spur,
    state::{AgentId, NodeKey},
};
use thiserror::Error;

use crate::state::{Agent, Busy};

use super::EnvNode;

pub struct LabelSet {
    pub mode: BusyMode,
    pub labels: Vec<Spur>,
    agents: Vec<AgentMapping>,
}

struct AgentMapping {
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

    /// Attempt to atomically claim the agent
    pub fn claim(&self) -> Option<Arc<Busy>> {
        // avoid needlessly upgrading the weak pointer
        if self.claim.strong_count() > 1 {
            return None;
        }

        let arc = self.claim.upgrade()?;
        // 2 because the agent owns arc, and this would be the second
        (Arc::strong_count(&arc) == 2).then_some(arc)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LabelSetError {
    #[error("agent {0} not found for node {1}")]
    AgentNotFound(AgentId, NodeKey),
    #[error("agent {0} already claimed for node {1}")]
    AgentAlreadyClaimed(AgentId, NodeKey),
    #[error("could not find any agents for node {0}")]
    NoAvailableAgents(NodeKey),
}

impl LabelSet {
    /// Create a new `LabelSet` from the given agents and labels
    pub fn new<'a, I: Iterator<Item = &'a Agent>>(
        mode: BusyMode,
        agents: I,
        labels: Vec<Spur>,
    ) -> Self {
        let agents = agents
            .filter_map(|agent| AgentMapping::new(mode, agent, &labels))
            .collect();

        Self {
            mode,
            labels,
            agents,
        }
    }

    /// Create a new `LabelSet` from all the labels in the given node config
    pub fn new_from_nodes<'a, I: Iterator<Item = &'a Agent>>(
        mode: BusyMode,
        agents: I,
        nodes: &IndexMap<NodeKey, EnvNode>,
    ) -> Self {
        let mut labels = HashSet::new();

        for node in nodes.values() {
            match node {
                EnvNode::Internal(n) => {
                    labels.extend(&n.labels);
                }
                EnvNode::External(_) => {}
            }
        }

        let labels: Vec<_> = labels.into_iter().collect();
        Self::new(mode, agents, labels)
    }

    /// Given a map of nodes, attempt to pair each node with an agent
    pub fn pair_with_nodes(
        &self,
        nodes: &IndexMap<NodeKey, EnvNode>,
    ) -> Result<impl Iterator<Item = (NodeKey, AgentId, Arc<Busy>)>, Vec<LabelSetError>> {
        // errors that occurred while pairing nodes with agents
        let (errors_tx, errors_rx) = mpsc::channel();
        // nodes that were successfully claimed. dropping this will automatically unclaim the agents
        let (claimed_tx, claimed_rx) = mpsc::channel();

        let (want_ids, want_labels) = nodes
            .iter()
            // filter out external nodes
            // split into nodes that want specific agents and nodes that want specific labels
            .filter_map(|(key, env_node)| match env_node {
                EnvNode::Internal(n) => match n.agent {
                    Some(agent) => Some((Some((key, agent)), None)),
                    None => Some((None, Some((key, n.mask(key, &self.labels))))),
                },
                EnvNode::External(_) => None,
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

        // handle the nodes that want specific agents first
        let agent_map = self
            .agents
            .iter()
            .map(|a| (a.id, a))
            .collect::<HashMap<_, _>>();

        // walk through all the nodes that want specific agents and attempt to pair them with an agent
        want_ids.into_par_iter().for_each(|(key, id)| {
            let Some(agent) = agent_map.get(&id) else {
                let _ = errors_tx.send(LabelSetError::AgentNotFound(id, key.clone()));
                return;
            };

            if let Some(claim) = agent.claim() {
                let _ = claimed_tx.send((key.clone(), id, claim));
            } else {
                let _ = errors_tx.send(LabelSetError::AgentAlreadyClaimed(id, key.clone()));
            }
        });

        // walk through all the nodes that want specific labels/modes and attempt to pair them with an agent
        // that has the matching mask
        want_labels.into_par_iter().for_each(|(key, mask)| {
            // find the first agent that can be claimed that fits the mask
            if let Some((id, claim)) = self.agents.iter().find_map(|a| {
                if a.mask.is_subset(&mask) {
                    a.claim().map(|c| (a.id, c))
                } else {
                    None
                }
            }) {
                let _ = claimed_tx.send((key.clone(), id, claim));
            } else {
                let _ = errors_tx.send(LabelSetError::NoAvailableAgents(key.clone()));
            }
        });

        let errors = errors_rx.try_iter().collect::<Vec<_>>();
        if errors.is_empty() {
            Ok(claimed_rx.into_iter())
        } else {
            Err(errors)
        }
    }
}
