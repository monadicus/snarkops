use indexmap::IndexSet;
use snops_common::state::{AgentId, CannonId, EnvId, NodeKey, StorageId};

use super::{EnvNode, EnvPeer, Environment};
use crate::{
    cannon::{sink::TxSink, source::TxSource},
    schema::nodes::{ExternalNode, Node},
};

pub struct PersistEnv {
    pub id: EnvId,
    /// A map of agents for looking them up later
    pub agents: IndexSet<AgentId>,
    pub storage_id: StorageId,
    /// List of nodes and their states or external node info
    pub nodes: Vec<(NodeKey, PersistNode)>,
    /// List of drains and the number of consumed lines
    pub tx_pipe_drains: Vec<String>,
    /// List of sink names
    pub tx_pipe_sinks: Vec<String>,
    /// Loaded cannon configs in this env
    pub cannon_configs: Vec<(CannonId, TxSource, TxSink)>,
}

impl From<&Environment> for PersistEnv {
    fn from(value: &Environment) -> Self {
        let agents: IndexSet<_> = value
            .node_map
            .right_values()
            .filter_map(|n| match n {
                EnvPeer::Internal(a) => Some(*a),
                EnvPeer::External(_) => None,
            })
            .collect();

        let nodes = value
            .initial_nodes
            .iter()
            .filter_map(|(k, v)| {
                let agent_index = value.node_map.get_by_left(k).and_then(|v| {
                    if let EnvPeer::Internal(a) = v {
                        agents.get_index_of(a).map(|i| i as u32)
                    } else {
                        None
                    }
                });
                match v {
                    EnvNode::Internal(n) => agent_index.map(|index| {
                        (
                            k.clone(),
                            PersistNode::Internal {
                                state: Box::new(n.clone()),
                                node_index: index,
                            },
                        )
                    }),
                    EnvNode::External(n) => Some((k.clone(), PersistNode::External(n.clone()))),
                }
            })
            .collect();

        PersistEnv {
            id: value.id,
            agents,
            storage_id: value.storage.id,
            nodes,
            tx_pipe_drains: value.tx_pipe.drains.keys().cloned().collect(),
            tx_pipe_sinks: value.tx_pipe.sinks.keys().cloned().collect(),
            cannon_configs: value
                .cannon_configs
                .iter()
                .map(|(k, (source, sink))| (*k, source.clone(), sink.clone()))
                .collect(),
        }
    }
}

pub enum PersistNode {
    Internal { node_index: u32, state: Box<Node> },
    External(ExternalNode),
}

pub struct PersistCannon {
    pub id: CannonId,
    pub env_id: EnvId,
    pub source: TxSource,
    pub sink: TxSink,
    pub tx_count: u64,
}
