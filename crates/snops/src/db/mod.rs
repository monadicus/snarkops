use std::path::PathBuf;

use self::{document::DbCollection, error::DatabaseError};

pub mod document;
pub mod error;

#[derive(Debug)]
pub struct Database {
    #[allow(unused)]
    pub(crate) db: sled::Db,
    /// Environment state, mapped by env id to env state
    pub(crate) envs: sled::Tree,
    /// Last known agent state, mapped by agent id to agent state
    pub(crate) agents: sled::Tree,
    /// Loaded storages, mapped by storage id to storage info
    pub(crate) storage: sled::Tree,
    /// Instanced cannons, mapped by (env id, cannon id) to cannon state
    #[allow(unused)]
    pub(crate) cannon_instances: sled::Tree,
    /// Shots of instanced cannons, mapped by (env id, cannon id) to shot count
    #[allow(unused)]
    pub(crate) cannon_counts: sled::Tree,
    /// Transaction drain counts, mapped by (env id, source id) to drain count
    pub(crate) tx_drain_counts: sled::Tree,
    /// Instanced timelines, mapped by timeline id to timeline state
    #[allow(unused)]
    pub(crate) timeline_instances: sled::Tree,
    /// Timeline outcome storage
    #[allow(unused)]
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
        let cannon_counts = db.open_tree(b"cannon_counts")?;
        let tx_drain_counts = db.open_tree(b"tx_drain_counts")?;
        let timeline_instances = db.open_tree(b"timeline")?;

        Ok(Self {
            db,
            envs,
            agents,
            storage,
            outcome_snapshots,
            cannon_instances,
            cannon_counts,
            tx_drain_counts,
            timeline_instances,
        })
    }

    /// Load the state of the object from the database at load time or return
    /// default
    pub fn load<T: DbCollection + Default>(&self) -> Result<T, DatabaseError> {
        T::restore(self)
    }
}
