use bytes::{Buf, BufMut};
use checkpoint::{CheckpointManager, RetentionPolicy};
use snops_common::{
    constant::LEDGER_BASE_DIR,
    format::{DataFormat, DataFormatReader},
    state::{AgentId, AgentState, InternedId, PortConfig, StorageId},
};
use tracing::info;

use super::{Agent, AgentPool};
use crate::{
    cli::Cli,
    db::{
        document::{load_interned_id, DbCollection, DbDocument},
        error::DatabaseError,
        Database,
    },
    schema::{
        error::StorageError,
        storage::{pick_commitee_addr, read_to_addrs, LoadedStorage, STORAGE_DIR},
    },
    server::jwt::Claims,
    state::{AgentAddrs, AgentFlags},
};

/// Metadata for storage that can be used to restore a loaded storage
pub struct PersistStorage {
    pub id: StorageId,
    pub version: u16,
    pub persist: bool,
    pub accounts: Vec<InternedId>,
    pub retention_policy: Option<RetentionPolicy>,
}

#[derive(Debug, Clone)]
pub struct PersistStorageFormatHeader {
    pub version: u8,
    pub retention_policy: <RetentionPolicy as DataFormat>::Header,
}

impl DataFormat for PersistStorageFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        Ok(self.version.write_data(writer)? + self.retention_policy.write_data(writer)?)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
                "PersistStorageFormatHeader",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        Ok(PersistStorageFormatHeader {
            version: reader.read_data(&())?,
            retention_policy: reader.read_data(&((), ()))?,
        })
    }
}

impl From<&LoadedStorage> for PersistStorage {
    fn from(storage: &LoadedStorage) -> Self {
        Self {
            id: storage.id,
            version: storage.version,
            persist: storage.persist,
            accounts: storage.accounts.keys().cloned().collect(),
            retention_policy: storage.checkpoints.as_ref().map(|c| c.policy().clone()),
        }
    }
}

impl PersistStorage {
    pub async fn load(self, cli: &Cli) -> Result<LoadedStorage, StorageError> {
        let id = self.id;
        let mut storage_path = cli.path.join(STORAGE_DIR);
        storage_path.push(id.to_string());
        let committee_file = storage_path.join("committee.json");

        let checkpoints = self
            .retention_policy
            .map(|policy| {
                CheckpointManager::load(storage_path.join(LEDGER_BASE_DIR), policy)
                    .map_err(StorageError::CheckpointManager)
            })
            .transpose()?;

        if let Some(checkpoints) = &checkpoints {
            info!("storage {id} checkpoint manager loaded {checkpoints}");
        } else {
            info!("storage {id} loaded without a checkpoint manager");
        }

        Ok(LoadedStorage {
            id,
            version: self.version,
            persist: self.persist,
            committee: read_to_addrs(pick_commitee_addr, &committee_file).await?,
            checkpoints,
            // TODO: waiting for #116 to be merged, then make a reusable function
            accounts: Default::default(),
        })
    }
}

impl DbCollection for AgentPool {
    fn restore(db: &Database) -> Result<Self, DatabaseError> {
        let map = AgentPool::default();
        for row in db.agents_old.iter() {
            let Some(id) = load_interned_id(row, "agent") else {
                continue;
            };

            match DbDocument::restore(db, id) {
                Ok(Some(agent)) => {
                    map.insert(id, agent);
                }
                // should be unreachable
                Ok(None) => {
                    tracing::error!("Agent {} not found in database", id);
                }
                Err(e) => {
                    tracing::error!("Error restoring agent {}: {}", id, e);
                }
            }
        }

        Ok(map)
    }
}

const AGENT_VERSION: u8 = 1;
impl DbDocument for Agent {
    type Key = AgentId;

    fn restore(db: &Database, key: Self::Key) -> Result<Option<Self>, DatabaseError> {
        let Some(raw) = db
            .agents_old
            .get(key)
            .map_err(|e| DatabaseError::LookupError(key.to_string(), "agents".to_owned(), e))?
        else {
            return Ok(None);
        };
        let mut buf = raw.as_ref();
        let version = buf.get_u8();
        if version != AGENT_VERSION {
            return Err(DatabaseError::UnsupportedVersion(
                key.to_string(),
                "agents".to_owned(),
                version,
            ));
        };

        let (state, nonce, flags, ports, addrs): (
            AgentState,
            u16,
            AgentFlags,
            Option<PortConfig>,
            Option<AgentAddrs>,
        ) = bincode::deserialize_from(&mut buf).map_err(|e| {
            DatabaseError::DeserializeError(key.to_string(), "agents".to_owned(), e)
        })?;

        let claims = Claims { id: key, nonce };

        Ok(Some(Agent::from_components(
            claims, state, flags, ports, addrs,
        )))
    }

    fn save(&self, db: &Database, key: Self::Key) -> Result<(), DatabaseError> {
        let mut buf = vec![];
        buf.put_u8(AGENT_VERSION);
        bincode::serialize_into(
            &mut buf,
            &(
                self.state(),
                self.claims().nonce,
                &self.flags,
                &self.ports,
                &self.addrs,
            ),
        )
        .map_err(|e| DatabaseError::SerializeError(key.to_string(), "agents".to_owned(), e))?;

        db.agents_old
            .insert(key, buf)
            .map_err(|e| DatabaseError::SaveError(key.to_string(), "agents".to_owned(), e))?;
        Ok(())
    }

    fn delete(db: &Database, key: Self::Key) -> Result<bool, DatabaseError> {
        Ok(db
            .agents_old
            .remove(key)
            .map_err(|e| DatabaseError::DeleteError(key.to_string(), "agents".to_owned(), e))?
            .is_some())
    }
}

impl DataFormat for PersistStorage {
    type Header = PersistStorageFormatHeader;
    const LATEST_HEADER: Self::Header = PersistStorageFormatHeader {
        version: 1,
        retention_policy: <RetentionPolicy as DataFormat>::LATEST_HEADER,
    };

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        let mut written = 0;

        written += self.id.write_data(writer)?;
        written += self.version.write_data(writer)?;
        written += self.persist.write_data(writer)?;
        written += self.accounts.write_data(writer)?;
        written += self.retention_policy.write_data(writer)?;

        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(snops_common::format::DataReadError::unsupported(
                "PersistStorage",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        Ok(PersistStorage {
            id: reader.read_data(&())?,
            version: reader.read_data(&())?,
            persist: reader.read_data(&())?,
            accounts: reader.read_data(&())?,
            retention_policy: reader.read_data(&header.retention_policy)?,
        })
    }
}
