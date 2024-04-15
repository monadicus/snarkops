use std::{collections::HashMap, str::FromStr, sync::Arc};

use bimap::BiMap;
use bytes::{Buf, BufMut};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use snops_common::state::{AgentId, CannonId, EnvId, NodeKey, StorageId, TxPipeId};

use super::{EnvError, EnvNode, EnvPeer, Environment, PrepareError, TxPipes};
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
        nodes::{ExternalNode, Node},
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
            .initial_nodes
            .iter()
            .filter_map(|(k, v)| {
                let agent_index = value.node_map.get_by_left(k).and_then(|v| {
                    if let EnvPeer::Internal(a) = v {
                        Some(a)
                    } else {
                        None
                    }
                });
                match v {
                    EnvNode::Internal(n) => agent_index.map(|agent| {
                        (
                            k.clone(),
                            PersistNode::Internal(*agent, Box::new(n.clone())),
                        )
                    }),
                    EnvNode::External(n) => Some((k.clone(), PersistNode::External(n.clone()))),
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
                .map(|(k, (source, sink))| (*k, source.clone(), sink.clone()))
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
        let mut initial_nodes = IndexMap::default();
        for (key, v) in self.nodes {
            match v {
                PersistNode::Internal(agent, n) => {
                    node_map.insert(key.clone(), EnvPeer::Internal(agent));
                    initial_nodes.insert(key, EnvNode::Internal(*n));
                }
                PersistNode::External(n) => {
                    node_map.insert(key.clone(), EnvPeer::External(key.clone()));
                    initial_nodes.insert(key, EnvNode::External(n));
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

        let mut cannon_configs = HashMap::new();
        for (k, source, sink) in self.cannon_configs {
            cannon_configs.insert(k, (source, sink));
        }

        Ok(Environment {
            id: self.id,
            storage: storage.clone(),
            node_map,
            initial_nodes,
            tx_pipe,
            cannon_configs,
            aot_bin: DEFAULT_AOT_BIN.clone(),
            cannons: Default::default(), // TODO: load cannons first

            // TODO: create persistence for these documents or move out of env
            outcomes: Default::default(),
            timeline: Default::default(),
            timeline_handle: Default::default(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistNode {
    Internal(AgentId, Box<Node>),
    External(ExternalNode),
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
        for row in db.envs.iter() {
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

const ENV_VERSION: u8 = 1;
impl DbDocument for PersistEnv {
    type Key = EnvId;

    fn restore(db: &Database, key: Self::Key) -> Result<Option<Self>, DatabaseError> {
        let Some(raw) = db
            .envs
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

        db.envs
            .insert(key, buf)
            .map_err(|e| DatabaseError::SaveError(key.to_string(), "env".to_owned(), e))?;
        Ok(())
    }

    fn delete(db: &Database, key: Self::Key) -> Result<bool, DatabaseError> {
        let res = db.envs.remove(key).map(|v| v.is_some())?;

        // remove drains associated with this env
        for (drain_id, _) in db.tx_drain_counts.scan_prefix(key).flatten() {
            if let Err(e) = db.tx_drain_counts.remove(drain_id) {
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

        let Some(raw) = db.tx_drain_counts.get(key_bin).map_err(|e| {
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
        db.tx_drain_counts
            .insert(key_bin, buf)
            .map_err(|e| DatabaseError::SaveError(key_str, "tx_pipe_drains".to_owned(), e))?;
        Ok(())
    }

    fn delete(db: &Database, key: Self::Key) -> Result<bool, DatabaseError> {
        let key_str = format!("{}.{}", key.0, key.1);
        let key_bin = concat_ids([key.0, key.1]);

        db.tx_drain_counts
            .remove(key_bin)
            .map_err(|e| DatabaseError::DeleteError(key_str, "tx_pipe_drains".to_owned(), e))
            .map(|v| v.is_some())
    }
}
