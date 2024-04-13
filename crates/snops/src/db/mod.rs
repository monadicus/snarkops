use std::path::PathBuf;

use self::{document::DbCollection, error::DatabaseError};

pub mod document;
pub mod error;

#[derive(Debug)]
pub struct Database {
    pub(crate) db: sled::Db,
    /// Environment state, mapped by env id to env state
    pub(crate) envs: sled::Tree,
    /// Last known agent state, mapped by agent id to agent state
    pub(crate) agents: sled::Tree,
    /// Loaded storages, mapped by storage id to storage info
    pub(crate) storage: sled::Tree,
    /// Instanced cannons, mapped by cannon id to cannon state
    pub(crate) cannon_instances: sled::Tree,
    /// Instanced timelines, mapped by timeline id to timeline state
    pub(crate) timeline_instances: sled::Tree,
    /// Timeline outcome storage
    pub(crate) outcome_snapshots: sled::Tree,
}

impl Database {
    pub fn open(path: &PathBuf) -> Result<Self, DatabaseError> {
        let db = sled::open(path)?;

        let envs = db.open_tree(b"env")?;
        let agents = db.open_tree(b"agent")?;
        let storage = db.open_tree(b"storage")?;
        let outcome_snapshots = db.open_tree(b"outcomes")?;
        let cannon_instances = db.open_tree(b"cannon")?;
        let timeline_instances = db.open_tree(b"timeline")?;

        Ok(Self {
            db,
            envs,
            agents,
            storage,
            outcome_snapshots,
            cannon_instances,
            timeline_instances,
        })
    }

    /// Load the state of the object from the database at load time or return
    /// default
    pub fn load<T: DbCollection + Default>(&self) -> Result<T, DatabaseError> {
        T::restore(self)
    }
}
