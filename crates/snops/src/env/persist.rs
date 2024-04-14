use indexmap::IndexSet;
use snops_common::state::{AgentId, CannonId, EnvId, NodeKey};

use crate::{
    cannon::{sink::TxSink, source::TxSource},
    schema::nodes::{ExternalNode, Node},
};

pub struct PersistEnv {
    pub id: EnvId,
    /// A map of agents for looking them up later
    pub agents: IndexSet<AgentId>,
    pub storage_id: Vec<String>,
    /// List of nodes and their states or external node info
    pub nodes: Vec<(NodeKey, PersistNode)>,
    /// List of drains and the number of consumed lines
    pub tx_pipe_drains: Vec<(String, u32)>,
    /// List of sink names
    pub tx_pipe_sinks: Vec<String>,
    /// Loaded cannon configs in this env
    pub cannon_configs: Vec<(String, TxSource, TxSink)>,
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
    pub fired_txs: u64,
    pub tx_count: u64,
}
