use std::path::Path;

use snops_common::{
    db::{error::DatabaseError, tree::DbTree, Database as DatabaseTrait},
    state::{AgentId, EnvId, NetworkId, StorageId},
};

use crate::{
    persist::{PersistEnv, PersistStorage},
    state::Agent,
};

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

impl DatabaseTrait for Database {
    fn open(path: &Path) -> Result<Self, DatabaseError> {
        let db = sled::open(path)?;
        let envs = DbTree::new(db.open_tree(b"v2/envs")?);
        let storage = DbTree::new(db.open_tree(b"v2/storage")?);
        let agents = DbTree::new(db.open_tree(b"v2/agents")?);

        Ok(Self {
            db,
            envs,
            storage,
            agents,
        })
    }
}
