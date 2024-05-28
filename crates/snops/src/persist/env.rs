use std::sync::Arc;

use bimap::BiMap;
use dashmap::DashMap;
use snops_common::state::{CannonId, EnvId, NetworkId, NodeKey, StorageId, TxPipeId};

use super::prelude::*;
use super::PersistNode;
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
    schema::storage::DEFAULT_AOT_BIN,
    state::StorageMap,
};

#[derive(Clone)]
pub struct PersistEnvFormatHeader {
    version: u8,
    nodes: DataHeaderOf<PersistNode>,
    tx_source: DataHeaderOf<TxSource>,
    tx_sink: DataHeaderOf<TxSink>,
    network: DataHeaderOf<NetworkId>,
}

pub struct PersistEnv {
    pub id: EnvId,
    pub storage_id: StorageId,
    pub network: NetworkId,
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
            network: value.network,
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
            .get(&(self.network, self.storage_id))
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
            network: self.network,
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

impl DataFormat for PersistEnvFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 2;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += writer.write_data(&self.version)?;
        written += write_dataformat(writer, &self.nodes)?;
        written += write_dataformat(writer, &self.tx_source)?;
        written += write_dataformat(writer, &self.tx_sink)?;
        written += writer.write_data(&self.network)?;
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header > Self::LATEST_HEADER || *header < 1 {
            return Err(DataReadError::unsupported(
                "PersistEnvHeader",
                format!("1 or {}", Self::LATEST_HEADER),
                header,
            ));
        }

        let version = reader.read_data(&())?;
        let nodes = read_dataformat(reader)?;
        let tx_source = read_dataformat(reader)?;
        let tx_sink = read_dataformat(reader)?;
        let network = if *header > 1 {
            reader.read_data(&())?
        } else {
            0
        };

        Ok(PersistEnvFormatHeader {
            version,
            nodes,
            tx_source,
            tx_sink,
            network,
        })
    }
}

impl DataFormat for PersistEnv {
    type Header = PersistEnvFormatHeader;
    const LATEST_HEADER: Self::Header = PersistEnvFormatHeader {
        version: 1,
        nodes: PersistNode::LATEST_HEADER, // TODO: use PersistNode::LATEST_HEADER
        tx_source: TxSource::LATEST_HEADER,
        tx_sink: TxSink::LATEST_HEADER,
        network: NetworkId::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;

        written += writer.write_data(&self.id)?;
        written += writer.write_data(&self.storage_id)?;
        written += writer.write_data(&self.nodes)?;
        written += writer.write_data(&self.tx_pipe_drains)?;
        written += writer.write_data(&self.tx_pipe_sinks)?;
        written += writer.write_data(&self.cannon_configs)?;
        written += writer.write_data(&self.network)?;

        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(DataReadError::unsupported(
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
        let network = if header.network > 0 {
            reader.read_data(&header.network)?
        } else {
            NetworkId::default()
        };

        Ok(PersistEnv {
            id,
            storage_id,
            network,
            nodes,
            tx_pipe_drains,
            tx_pipe_sinks,
            cannon_configs,
        })
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use snops_common::{
        format::{read_dataformat, write_dataformat, DataFormat},
        state::{InternedId, NetworkId},
    };

    use crate::{
        cannon::{sink::TxSink, source::TxSource},
        persist::{
            PersistEnv, PersistEnvFormatHeader, PersistNode, PersistNodeFormatHeader,
            TxSinkFormatHeader, TxSourceFormatHeader,
        },
    };

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr, $b:expr) => {
            #[test]
            fn $name() -> Result<(), Box<dyn std::error::Error>> {
                let mut data = Vec::new();
                write_dataformat(&mut data, &$a)?;
                assert_eq!(data, $b);

                let mut reader = &data[..];
                let read_value = read_dataformat::<_, $ty>(&mut reader)?;

                // write the data again because not every type implements PartialEq
                let mut data2 = Vec::new();
                write_dataformat(&mut data2, &read_value)?;
                assert_eq!(data, data2);
                Ok(())
            }
        };
    }

    case!(
        env_header,
        PersistEnvFormatHeader,
        PersistEnv::LATEST_HEADER,
        [
            PersistEnvFormatHeader::LATEST_HEADER.to_byte_vec()?,
            PersistEnv::LATEST_HEADER.version.to_byte_vec()?,
            PersistNodeFormatHeader::LATEST_HEADER.to_byte_vec()?,
            PersistNode::LATEST_HEADER.to_byte_vec()?,
            TxSourceFormatHeader::LATEST_HEADER.to_byte_vec()?,
            TxSource::LATEST_HEADER.to_byte_vec()?,
            TxSinkFormatHeader::LATEST_HEADER.to_byte_vec()?,
            TxSink::LATEST_HEADER.to_byte_vec()?,
            NetworkId::LATEST_HEADER.to_byte_vec()?,
        ]
        .concat()
    );

    case!(
        env,
        PersistEnv,
        PersistEnv {
            id: InternedId::from_str("foo")?,
            storage_id: InternedId::from_str("bar")?,
            network: Default::default(),
            nodes: Default::default(),
            tx_pipe_drains: Default::default(),
            tx_pipe_sinks: Default::default(),
            cannon_configs: Default::default()
        },
        [
            PersistEnvFormatHeader::LATEST_HEADER.to_byte_vec()?,
            PersistEnv::LATEST_HEADER.to_byte_vec()?,
            InternedId::from_str("foo")?.to_byte_vec()?,
            InternedId::from_str("bar")?.to_byte_vec()?,
            Vec::<(String, PersistNode)>::new().to_byte_vec()?,
            Vec::<InternedId>::new().to_byte_vec()?,
            Vec::<InternedId>::new().to_byte_vec()?,
            Vec::<(InternedId, TxSource, TxSink)>::new().to_byte_vec()?,
            NetworkId::default().to_byte_vec()?,
        ]
        .concat()
    );
}
