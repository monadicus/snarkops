use std::path::PathBuf;

use snops_common::state::{AgentId, EnvId, NetworkId, StorageId};

use self::{error::DatabaseError, tree::DbTree};
use crate::{
    persist::{PersistEnv, PersistStorage},
    state::Agent,
};

pub mod error;
pub mod tree;

pub struct Database {
    #[allow(unused)]
    pub(crate) db: sled::Db,

    /// Environment state, mapped by env id to env state
    pub(crate) envs: DbTree<EnvId, PersistEnv>,
    /// Storage state, mapped by storage id to storage state
    pub(crate) storage: DbTree<(NetworkId, StorageId), PersistStorage>,
    /// Last known agent state, mapped by agent id to agent state
    pub(crate) agents: DbTree<AgentId, Agent>,
}

impl Database {
    pub fn open(path: &PathBuf) -> Result<Self, DatabaseError> {
        let db = sled::open(path)?;

        Ok(Self {
            envs: DbTree::new(db.open_tree(b"v2/envs")?),
            storage: DbTree::new(db.open_tree(b"v2/storage")?),
            agents: DbTree::new(db.open_tree(b"v2/agents")?),

            db,
        })
    }
}
