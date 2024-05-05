use std::sync::Arc;

use bimap::BiMap;
use dashmap::DashMap;
use snops_common::{
    format::{read_dataformat, write_dataformat, DataFormat, DataFormatReader, DataFormatWriter},
    state::{CannonId, EnvId, NodeKey, StorageId, TxPipeId},
};

use super::{PersistNode, PersistNodeFormatHeader};
use crate::{
    cannon::{
        file::{TransactionDrain, TransactionSink},
        sink::TxSink,
        source::TxSource,
    },
    cli::Cli,
    db::Database,
    env::{
        error::{EnvError, PrepareError},
        EnvNodeState, EnvPeer, Environment, TxPipes,
    },
    persist::{TxSinkFormatHeader, TxSourceFormatHeader},
    schema::storage::DEFAULT_AOT_BIN,
    state::StorageMap,
};

#[derive(Clone)]
pub struct PersistEnvHeader {
    version: u8,
    nodes: PersistNodeFormatHeader,
    tx_source: TxSourceFormatHeader,
    tx_sink: TxSinkFormatHeader,
}

pub struct PersistEnv {
    pub id: EnvId,
    pub storage_id: StorageId,
    /// List of nodes and their states or external node info
    pub nodes: Vec<(NodeKey, PersistNode)>,
    /// List of drains and the number of consumed lines
    pub tx_pipe_drains: Vec<TxPipeId>,
    /// List of sink names
    pub tx_pipe_sinks: Vec<TxPipeId>,
    /// Loaded cannon configs in this env
    pub cannon_configs: Vec<(CannonId, TxSource, TxSink)>,
}

impl From<&Environment> for PersistEnv {
    fn from(value: &Environment) -> Self {
        let nodes = value
            .node_states
            .iter()
            .filter_map(|entry| {
                let key = entry.key();
                let agent_index = value.node_peers.get_by_left(key).and_then(|v| {
                    if let EnvPeer::Internal(a) = v {
                        Some(a)
                    } else {
                        None
                    }
                });
                match entry.value() {
                    EnvNodeState::Internal(n) => agent_index.map(|agent| {
                        (
                            key.clone(),
                            PersistNode::Internal(*agent, Box::new(n.clone())),
                        )
                    }),
                    EnvNodeState::External(n) => {
                        Some((key.clone(), PersistNode::External(n.clone())))
                    }
                }
            })
            .collect();

        PersistEnv {
            id: value.id,
            storage_id: value.storage.id,
            nodes,
            tx_pipe_drains: value.tx_pipe.drains.keys().cloned().collect(),
            tx_pipe_sinks: value.tx_pipe.sinks.keys().cloned().collect(),
            cannon_configs: value
                .cannon_configs
                .iter()
                .map(|v| (*v.key(), v.0.clone(), v.1.clone()))
                .collect(),
        }
    }
}

impl PersistEnv {
    pub async fn load(
        self,
        db: &Database,
        storage: &StorageMap,
        cli: &Cli,
    ) -> Result<Environment, EnvError> {
        let storage = storage
            .get(&self.storage_id)
            .ok_or(PrepareError::MissingStorage)?;

        let mut node_map = BiMap::default();
        let initial_nodes = DashMap::default();
        for (key, v) in self.nodes {
            match v {
                PersistNode::Internal(agent, n) => {
                    node_map.insert(key.clone(), EnvPeer::Internal(agent));
                    initial_nodes.insert(key, EnvNodeState::Internal(*n));
                }
                PersistNode::External(n) => {
                    node_map.insert(key.clone(), EnvPeer::External(key.clone()));
                    initial_nodes.insert(key, EnvNodeState::External(n));
                }
            }
        }

        let mut tx_pipe = TxPipes::default();
        for drain_id in self.tx_pipe_drains {
            let count = match db.tx_drain_counts.restore(&(self.id, drain_id)) {
                Ok(Some(count)) => count.count,
                Ok(None) => 0,
                Err(e) => {
                    tracing::error!("Error loading drain count for {}/{drain_id}: {e}", self.id);
                    0
                }
            };

            tx_pipe.drains.insert(
                drain_id,
                Arc::new(TransactionDrain::new(
                    storage.path_cli(cli),
                    drain_id,
                    count,
                )?),
            );
        }
        for sink_id in self.tx_pipe_sinks {
            tx_pipe.sinks.insert(
                sink_id,
                Arc::new(TransactionSink::new(storage.path_cli(cli), sink_id)?),
            );
        }

        let cannon_configs = DashMap::new();
        for (k, source, sink) in self.cannon_configs {
            cannon_configs.insert(k, (source, sink));
        }

        Ok(Environment {
            id: self.id,
            storage: storage.clone(),
            node_peers: node_map,
            node_states: initial_nodes,
            tx_pipe,
            cannon_configs,
            aot_bin: DEFAULT_AOT_BIN.clone(),
            cannons: Default::default(), // TODO: load cannons first

            // TODO: create persistence for these documents or move out of env
            outcomes: Default::default(),
            timelines: Default::default(),
            timeline_handle: Default::default(),
        })
    }
}

impl DataFormat for PersistEnvHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        let mut written = 0;
        written += writer.write_data(&self.version)?;
        written += write_dataformat(writer, &self.nodes)?;
        written += write_dataformat(writer, &self.tx_source)?;
        written += write_dataformat(writer, &self.tx_sink)?;
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
                "PersistEnvHeader",
                Self::LATEST_HEADER,
                header,
            ));
        }

        let version = reader.read_data(&())?;
        let nodes = read_dataformat(reader)?;
        let tx_source = read_dataformat(reader)?;
        let tx_sink = read_dataformat(reader)?;

        Ok(PersistEnvHeader {
            version,
            nodes,
            tx_source,
            tx_sink,
        })
    }
}

impl DataFormat for PersistEnv {
    type Header = PersistEnvHeader;
    const LATEST_HEADER: Self::Header = PersistEnvHeader {
        version: 1,
        nodes: PersistNode::LATEST_HEADER, // TODO: use PersistNode::LATEST_HEADER
        tx_source: TxSource::LATEST_HEADER,
        tx_sink: TxSink::LATEST_HEADER,
    };

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        let mut written = 0;

        written += writer.write_data(&self.storage_id)?;
        written += writer.write_data(&self.nodes)?;
        written += writer.write_data(&self.cannon_configs)?;

        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(snops_common::format::DataReadError::unsupported(
                "PersistEnv",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        let id = reader.read_data(&())?;
        let storage_id = reader.read_data(&())?;
        let nodes = reader.read_data(&(header.tx_source.node_key, header.nodes.clone()))?;
        let tx_pipe_drains = reader.read_data(&())?;
        let tx_pipe_sinks = reader.read_data(&())?;
        let cannon_configs =
            reader.read_data(&((), header.tx_source.clone(), header.tx_sink.clone()))?;

        Ok(PersistEnv {
            id,
            storage_id,
            nodes,
            tx_pipe_drains,
            tx_pipe_sinks,
            cannon_configs,
        })
    }
}
