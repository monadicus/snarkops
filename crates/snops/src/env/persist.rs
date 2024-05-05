use std::{str::FromStr, sync::Arc};

use bimap::BiMap;
use bytes::{Buf, BufMut};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use snops_common::{
    format::{read_dataformat, write_dataformat, DataFormat, DataFormatReader, DataFormatWriter},
    state::{AgentId, CannonId, EnvId, NodeKey, StorageId, TxPipeId},
};

use super::{EnvError, EnvNodeState, EnvPeer, Environment, PrepareError, TxPipes};
use crate::{
    cannon::{
        file::{TransactionDrain, TransactionSink},
        sink::TxSink,
        source::TxSource,
    },
    cli::Cli,
    db::{
        document::{concat_ids, load_interned_id, BEncDec, DbCollection, DbDocument},
        error::DatabaseError,
        Database,
    },
    impl_bencdec_serde,
    schema::{
        nodes::{ExternalNode, Node, NodeFormatHeader},
        storage::DEFAULT_AOT_BIN,
    },
    state::StorageMap,
};

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
            let count = match PersistDrainCount::restore(db, (self.id, drain_id)) {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistNode {
    Internal(AgentId, Box<Node>),
    External(ExternalNode),
}

#[derive(Debug, Clone)]
pub struct PersistNodeFormatHeader {
    pub(crate) node: NodeFormatHeader,
    pub(crate) external_node: <ExternalNode as DataFormat>::Header,
}

impl DataFormat for PersistNodeFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        Ok(write_dataformat(writer, &self.node)? + write_dataformat(writer, &self.external_node)?)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
                "PersistNodeFormatHeader",
                Self::LATEST_HEADER,
                header,
            ));
        }

        let node = read_dataformat(reader)?;
        let external_node = read_dataformat(reader)?;

        Ok(PersistNodeFormatHeader {
            node,
            external_node,
        })
    }
}

impl DataFormat for PersistNode {
    type Header = PersistNodeFormatHeader;
    const LATEST_HEADER: Self::Header = PersistNodeFormatHeader {
        node: Node::LATEST_HEADER,
        external_node: <ExternalNode as DataFormat>::LATEST_HEADER,
    };

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        let mut written = 0;
        match self {
            PersistNode::Internal(id, state) => {
                written += writer.write_data(&0u8)?;
                written += writer.write_data(id)?;
                written += writer.write_data(state)?;
            }
            PersistNode::External(n) => {
                written += writer.write_data(&1u8)?;
                written += writer.write_data(n)?;
            }
        }
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        match reader.read_data(&())? {
            0u8 => {
                let id = reader.read_data(&())?;
                let state = reader.read_data(&header.node)?;
                Ok(PersistNode::Internal(id, Box::new(state)))
            }
            1u8 => {
                let n = reader.read_data(&header.external_node)?;
                Ok(PersistNode::External(n))
            }
            n => Err(snops_common::format::DataReadError::Custom(format!(
                "invalid PersistNode discriminant: {n}"
            ))),
        }
    }
}

impl_bencdec_serde!(PersistNode);

// ExternalNode's deserializer is tailored specifically to YAML and uses
// deserialize_any, which is not supported by bincode.
impl BEncDec for PersistNode {
    fn as_bytes(&self) -> bincode::Result<Vec<u8>> {
        let mut buf = Vec::new();
        match self {
            PersistNode::Internal(id, state) => {
                buf.put_u8(0);

                let id_bytes: &[u8] = id.as_ref();
                buf.put_u8(id_bytes.len() as u8);
                buf.extend_from_slice(id_bytes);

                state.write_bytes(&mut buf)?;
            }
            PersistNode::External(n) => {
                buf.put_u8(1);
                n.write_bytes(&mut buf)?;
            }
        }
        Ok(buf)
    }

    fn from_bytes(mut bytes: &[u8]) -> bincode::Result<Self> {
        if bytes.is_empty() {
            return Err(bincode::ErrorKind::Custom("end of input".to_owned()).into());
        }

        match bytes.get_u8() {
            0 => {
                let id_len = bytes.get_u8() as usize;
                if bytes.len() < id_len {
                    return Err(
                        bincode::ErrorKind::Custom("invalid agent id length".to_owned()).into(),
                    );
                }
                let id =
                    AgentId::from_str(std::str::from_utf8(&bytes[..id_len]).map_err(|_| {
                        bincode::ErrorKind::Custom("agent id not utf-8 string".to_owned())
                    })?)
                    .map_err(|_| bincode::ErrorKind::Custom("invalid agent id".to_owned()))?;
                bytes.advance(id_len);
                let state = Node::read_bytes(&mut bytes)?;
                Ok(PersistNode::Internal(id, Box::new(state)))
            }
            1 => {
                let n = ExternalNode::read_bytes(&mut bytes)?;
                Ok(PersistNode::External(n))
            }
            _ => Err(bincode::ErrorKind::Custom("invalid node kind".to_owned()).into()),
        }
    }
}

pub struct PersistDrainCount {
    pub count: u32,
}

impl DbCollection for Vec<PersistEnv> {
    fn restore(db: &Database) -> Result<Self, DatabaseError> {
        let mut vec = Vec::new();
        for row in db.envs_old.iter() {
            let Some(id) = load_interned_id(row, "env") else {
                continue;
            };

            match DbDocument::restore(db, id) {
                Ok(Some(storage)) => {
                    vec.push(storage);
                }
                // should be unreachable
                Ok(None) => {
                    tracing::error!("Env {} not found in database", id);
                }
                Err(e) => {
                    tracing::error!("Error restoring env {}: {}", id, e);
                }
            }
        }
        Ok(vec)
    }
}

#[derive(Clone)]
pub struct PersistEnvHeader {
    env: u8,
    nodes: PersistNodeFormatHeader,
    tx_pipe_drains: u8,
    tx_pipe_sinks: u8,
    cannon_configs: u8,
}

impl DataFormat for PersistEnvHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        let mut written = 0;
        written += writer.write_data(&self.env)?;
        written += write_dataformat(writer, &self.nodes)?;
        written += writer.write_data(&self.tx_pipe_drains)?;
        written += writer.write_data(&self.tx_pipe_sinks)?;
        written += writer.write_data(&self.cannon_configs)?;
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

        let env = reader.read_data(&())?;
        let nodes = read_dataformat(reader)?;
        let tx_pipe_drains = reader.read_data(&())?;
        let tx_pipe_sinks = reader.read_data(&())?;
        let cannon_configs = reader.read_data(&())?;

        Ok(PersistEnvHeader {
            env,
            nodes,
            tx_pipe_drains,
            tx_pipe_sinks,
            cannon_configs,
        })
    }
}

impl DataFormat for PersistEnv {
    type Header = PersistEnvHeader;
    const LATEST_HEADER: Self::Header = PersistEnvHeader {
        env: 1,
        nodes: PersistNode::LATEST_HEADER, // TODO: use PersistNode::LATEST_HEADER
        tx_pipe_drains: 1,
        tx_pipe_sinks: 1,
        cannon_configs: 1,
    };

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        let mut written = 0;

        written += writer.write_data(&self.storage_id)?;
        written += writer.write_data(&self.nodes)?; // TODO impl
        written += writer.write_data(&self.tx_pipe_drains)?;
        written += writer.write_data(&self.tx_pipe_sinks)?;
        // written += writer.write_data(&self.cannon_configs)?; // TODO impl

        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if header.env != Self::LATEST_HEADER.env {
            return Err(snops_common::format::DataReadError::unsupported(
                "PersistEnv",
                Self::LATEST_HEADER.env,
                header.env,
            ));
        }

        let id = reader.read_data(&())?;
        let storage_id = reader.read_data(&())?;
        // let nodes = reader.read_data(&())?;  // TODO impl
        let tx_pipe_drains = reader.read_data(&())?;
        let tx_pipe_sinks = reader.read_data(&())?;
        // let cannon_configs = reader.read_data(&())?;  // TODO impl

        Ok(PersistEnv {
            id,
            storage_id,
            nodes: vec![], // TODO impl
            tx_pipe_drains,
            tx_pipe_sinks,
            cannon_configs: vec![], // TODO impl
        })
    }
}

const ENV_VERSION: u8 = 1;
impl DbDocument for PersistEnv {
    type Key = EnvId;

    fn restore(db: &Database, key: Self::Key) -> Result<Option<Self>, DatabaseError> {
        let Some(raw) = db
            .envs_old
            .get(key)
            .map_err(|e| DatabaseError::LookupError(key.to_string(), "env".to_owned(), e))?
        else {
            return Ok(None);
        };

        let mut buf = raw.as_ref();
        let version = buf.get_u8();
        if version != ENV_VERSION {
            return Err(DatabaseError::UnsupportedVersion(
                key.to_string(),
                "env".to_owned(),
                version,
            ));
        };

        let (storage_id, nodes, tx_pipe_drains, tx_pipe_sinks, cannon_configs) =
            bincode::deserialize(buf).map_err(|e| {
                DatabaseError::DeserializeError(key.to_string(), "env".to_owned(), e)
            })?;

        Ok(Some(PersistEnv {
            id: key,
            storage_id,
            nodes,
            tx_pipe_drains,
            tx_pipe_sinks,
            cannon_configs,
        }))
    }

    fn save(&self, db: &Database, key: Self::Key) -> Result<(), DatabaseError> {
        let mut buf = vec![];
        buf.put_u8(ENV_VERSION);

        bincode::serialize_into(
            &mut buf,
            &(
                &self.storage_id,
                &self.nodes,
                &self.tx_pipe_drains,
                &self.tx_pipe_sinks,
                &self.cannon_configs,
            ),
        )
        .map_err(|e| DatabaseError::SerializeError(key.to_string(), "env".to_owned(), e))?;

        db.envs_old
            .insert(key, buf)
            .map_err(|e| DatabaseError::SaveError(key.to_string(), "env".to_owned(), e))?;
        Ok(())
    }

    fn delete(db: &Database, key: Self::Key) -> Result<bool, DatabaseError> {
        let res = db.envs_old.remove(key).map(|v| v.is_some())?;

        // remove drains associated with this env
        for (drain_id, _) in db.tx_drain_counts_old.scan_prefix(key).flatten() {
            if let Err(e) = db.tx_drain_counts_old.remove(drain_id) {
                tracing::error!("Error deleting tx_pipe_drains for env {}: {e}", key);
            }
        }

        Ok(res)
    }
}

const DRAIN_COUNT_VERSION: u8 = 1;
impl DbDocument for PersistDrainCount {
    type Key = (EnvId, TxPipeId);

    fn restore(db: &Database, key: Self::Key) -> Result<Option<Self>, DatabaseError> {
        let key_str = format!("{}.{}", key.0, key.1);
        let key_bin = concat_ids([key.0, key.1]);

        let Some(raw) = db.tx_drain_counts_old.get(key_bin).map_err(|e| {
            DatabaseError::LookupError(key_str.clone(), "tx_pipe_drains".to_owned(), e)
        })?
        else {
            return Ok(None);
        };

        let mut buf = raw.as_ref();
        let version = buf.get_u8();
        if version != DRAIN_COUNT_VERSION {
            return Err(DatabaseError::UnsupportedVersion(
                key_str,
                "tx_pipe_drains".to_owned(),
                version,
            ));
        };

        let count = buf.get_u32();
        Ok(Some(PersistDrainCount { count }))
    }

    fn save(&self, db: &Database, key: Self::Key) -> Result<(), DatabaseError> {
        let key_str = format!("{}.{}", key.0, key.1);
        let key_bin = concat_ids([key.0, key.1]);

        let mut buf = vec![];
        buf.put_u8(DRAIN_COUNT_VERSION);
        buf.put_u32(self.count);
        db.tx_drain_counts_old
            .insert(key_bin, buf)
            .map_err(|e| DatabaseError::SaveError(key_str, "tx_pipe_drains".to_owned(), e))?;
        Ok(())
    }

    fn delete(db: &Database, key: Self::Key) -> Result<bool, DatabaseError> {
        let key_str = format!("{}.{}", key.0, key.1);
        let key_bin = concat_ids([key.0, key.1]);

        db.tx_drain_counts_old
            .remove(key_bin)
            .map_err(|e| DatabaseError::DeleteError(key_str, "tx_pipe_drains".to_owned(), e))
            .map(|v| v.is_some())
    }
}
