use std::str::FromStr;

use bytes::{Buf, BufMut};
use snops_common::state::{AgentId, AgentState, PortConfig};

use super::{Agent, AgentPool};
use crate::{
    db::{
        document::{DbCollection, DbDocument},
        error::DatabaseError,
        Database,
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

        Ok(Some(Agent::restore(claims, state, flags, ports, addrs)))
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
