use std::str::FromStr;

use bytes::{Buf, BufMut};
use checkpoint::{CheckpointManager, RetentionPolicy};
use snops_common::{
    constant::LEDGER_BASE_DIR,
    state::{AgentId, AgentState, PortConfig},
};
use tracing::info;

use super::{Agent, AgentPool};
use crate::{
    cli::Cli,
    db::{
        document::{DbCollection, DbDocument},
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

impl DbCollection for AgentPool {
    fn restore(db: &Database) -> Result<Self, DatabaseError> {
        let mut map = AgentPool::default();
        for row in db.agents.iter() {
            let id = match row {
                Ok((key_bytes, _)) => {
                    let key_str = match std::str::from_utf8(&key_bytes) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("Error reading agent key: {}", e);
                            continue;
                        }
                    };

                    match AgentId::from_str(key_str) {
                        Ok(key_id) => key_id,
                        Err(e) => {
                            tracing::error!("Error parsing agent key: {}", e);
                            continue;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error reading agent row: {}", e);
                    continue;
                }
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

impl DbDocument for Agent {
    type Key = AgentId;

    fn restore(db: &Database, key: Self::Key) -> Result<Option<Self>, DatabaseError> {
        let Some(raw) = db
            .agents
            .get(key)
            .map_err(|e| DatabaseError::LookupError(key.to_string(), "agents".to_owned(), e))?
        else {
            return Ok(None);
        };
        let mut buf = raw.as_ref();
        let version = buf.get_u8();
        if version != 1 {
            return Err(DatabaseError::UnsupportedVersion(
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
        buf.put_u8(1);
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
        db.agents
            .insert(key, buf)
            .map_err(|e| DatabaseError::SaveError(key.to_string(), "agents".to_owned(), e))?;
        Ok(())
    }

    fn delete(&self, db: &Database, key: Self::Key) -> Result<bool, DatabaseError> {
        Ok(db
            .agents
            .remove(key)
            .map_err(|e| DatabaseError::DeleteError(key.to_string(), "agents".to_owned(), e))?
            .is_some())
    }
}

impl DbCollection for Vec<PersistStorage> {
    fn restore(db: &Database) -> Result<Self, DatabaseError> {
        let mut vec = Vec::new();
        for row in db.storage.iter() {
            let id = match row {
                Ok((key_bytes, _)) => {
                    let key_str = match std::str::from_utf8(&key_bytes) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("Error reading storage key: {}", e);
                            continue;
                        }
                    };

                    key_str.to_string()
                }
                Err(e) => {
                    tracing::error!("Error reading storage row: {}", e);
                    continue;
                }
            };

            match DbDocument::restore(db, id.clone()) {
                Ok(Some(storage)) => {
                    vec.push(storage);
                }
                // should be unreachable
                Ok(None) => {
                    tracing::error!("Storage {} not found in database", id);
                }
                Err(e) => {
                    tracing::error!("Error restoring storage {}: {}", id, e);
                }
            }
        }

        Ok(vec)
    }
}

/// Metadata for storage that can be used to restore a loaded storage
pub struct PersistStorage {
    pub id: String,
    pub version: u16,
    pub persist: bool,
    pub accounts: Vec<String>,
    pub retention_policy: Option<RetentionPolicy>,
}

impl From<&LoadedStorage> for PersistStorage {
    fn from(storage: &LoadedStorage) -> Self {
        Self {
            id: storage.id.clone(),
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
        let storage_path = cli.path.join(STORAGE_DIR).join(&id);
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

impl DbDocument for PersistStorage {
    type Key = String;

    fn restore(db: &Database, key: Self::Key) -> Result<Option<Self>, DatabaseError> {
        let Some(raw) = db
            .storage
            .get(&key)
            .map_err(|e| DatabaseError::LookupError(key.clone(), "storage".to_owned(), e))?
        else {
            return Ok(None);
        };

        let mut buf = raw.as_ref();
        let version = buf.get_u8();
        if version != 1 {
            return Err(DatabaseError::UnsupportedVersion(
                "storage".to_owned(),
                version,
            ));
        };

        let (storage_version, accounts, persist, retention_policy) =
            bincode::deserialize_from(&mut buf).map_err(|e| {
                DatabaseError::DeserializeError(key.clone(), "storage".to_owned(), e)
            })?;

        Ok(Some(PersistStorage {
            id: key,
            version: storage_version,
            accounts,
            persist,
            retention_policy,
        }))
    }

    fn save(&self, db: &Database, key: Self::Key) -> Result<(), DatabaseError> {
        let mut buf = vec![];
        buf.put_u8(1);
        bincode::serialize_into(
            &mut buf,
            &(
                self.version,
                &self.accounts,
                self.persist,
                &self.retention_policy,
            ),
        )
        .map_err(|e| DatabaseError::SerializeError(key.clone(), "storage".to_owned(), e))?;
        db.storage
            .insert(&key, buf)
            .map_err(|e| DatabaseError::SaveError(key, "storage".to_owned(), e))?;
        Ok(())
    }

    fn delete(&self, db: &Database, key: Self::Key) -> Result<bool, DatabaseError> {
        Ok(db
            .storage
            .remove(&key)
            .map_err(|e| DatabaseError::DeleteError(key, "storage".to_owned(), e))?
            .is_some())
    }
}
