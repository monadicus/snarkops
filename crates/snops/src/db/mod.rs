use std::path::PathBuf;

use snops_common::state::{EnvId, TxPipeId};

use self::{document::DbCollection, error::DatabaseError, tree::DbTree};
use crate::{
    env::persist::{PersistDrainCount, PersistEnv},
    state::persist::PersistStorage,
};

pub mod document;
pub mod error;
pub mod tree;

pub struct Database {
    #[allow(unused)]
    pub(crate) db: sled::Db,

    /// Environment state, mapped by env id to env state
    pub(crate) envs: DbTree<EnvId, PersistEnv>,
    /// Transaction drain counts, mapped by (env d, source id) to drain count
    pub(crate) tx_drain_counts: DbTree<(EnvId, TxPipeId), PersistDrainCount>,
    /// Storage state, mapped by storage id to storage state
    pub(crate) storage: DbTree<EnvId, PersistStorage>,

    /// Last known agent state, mapped by agent id to agent state
    pub(crate) agents_old: sled::Tree,
    /// Instanced cannons, mapped by (env id, cannon id) to cannon state
    #[allow(unused)]
    pub(crate) cannon_instances_old: sled::Tree,
    /// Shots of instanced cannons, mapped by (env id, cannon id) to shot count
    #[allow(unused)]
    pub(crate) cannon_counts_old: sled::Tree,
    /// Instanced timelines, mapped by timeline id to timeline state
    #[allow(unused)]
    pub(crate) timeline_instances_old: sled::Tree,
    /// Timeline outcome storage
    #[allow(unused)]
    pub(crate) outcome_snapshots_old: sled::Tree,
}

impl Database {
    pub fn open(path: &PathBuf) -> Result<Self, DatabaseError> {
        let db = sled::open(path)?;

        let agents_old = db.open_tree(b"agent")?;
        let outcome_snapshots_old = db.open_tree(b"outcomes")?;
        let cannon_instances_old = db.open_tree(b"cannon")?;
        let cannon_counts_old = db.open_tree(b"cannon_counts")?;
        let timeline_instances_old = db.open_tree(b"timeline")?;

        Ok(Self {
            envs: DbTree::new(db.open_tree(b"v2/envs")?),
            tx_drain_counts: DbTree::new(db.open_tree(b"v2/tx_drain_counts")?),
            storage: DbTree::new(db.open_tree(b"v2/storage")?),

            agents_old,
            outcome_snapshots_old,
            cannon_instances_old,
            cannon_counts_old,
            timeline_instances_old,

            db,
        })
    }

    /// Load the state of the object from the database at load time or return
    /// default
    pub fn load<T: DbCollection + Default>(&self) -> Result<T, DatabaseError> {
        T::restore(self)
    }
}
