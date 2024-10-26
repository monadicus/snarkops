use std::path::Path;

use snops_common::{
    aot_cmds::Authorization,
    db::{error::DatabaseError, tree::DbTree, Database as DatabaseTrait},
    format::PackedUint,
    state::{AgentId, CannonId, EnvId, NetworkId, StorageId},
};

use crate::{
    cannon::status::TransactionSendState,
    persist::{PersistEnv, PersistStorage},
    state::Agent,
};

pub type TxEntry = (EnvId, CannonId, String);

pub struct Database {
    #[allow(unused)]
    pub(crate) db: sled::Db,

    /// Environment state, mapped by env id to env state
    pub(crate) envs: DbTree<EnvId, PersistEnv>,
    /// Storage state, mapped by storage id to storage state
    pub(crate) storage: DbTree<(NetworkId, StorageId), PersistStorage>,
    /// Last known agent state, mapped by agent id to agent state
    pub(crate) agents: DbTree<AgentId, Agent>,
    /// Temporary storage for cannon authorizations to prevent data loss
    pub(crate) tx_auths: DbTree<TxEntry, Authorization>,
    /// Temporary storage for cannon executed transactions to ensure they are
    /// not ghosted
    pub(crate) tx_blobs: DbTree<TxEntry, serde_json::Value>,
    /// Status tracking of transactions managed by a single cannon
    pub(crate) tx_status: DbTree<TxEntry, TransactionSendState>,
    /// Index tracking for transactions, used for ordering. The empty string key
    /// is used to track the last transaction index.
    ///
    /// Transactions with lower indices are prioritized for execution and
    /// broadcast.
    pub(crate) tx_index: DbTree<TxEntry, PackedUint>,
    /// Number of attempts for the transaction's current state
    pub(crate) tx_attempts: DbTree<TxEntry, PackedUint>,
    // TODO: tx_attempts for tracking retries (of broadcast and execution)
}

impl DatabaseTrait for Database {
    fn open(path: &Path) -> Result<Self, DatabaseError> {
        let db = sled::open(path)?;
        let envs = DbTree::new(db.open_tree(b"v2/envs")?);
        let storage = DbTree::new(db.open_tree(b"v2/storage")?);
        let agents = DbTree::new(db.open_tree(b"v2/agents")?);
        let tx_auths = DbTree::new(db.open_tree(b"v2/tx_auths")?);
        let tx_blobs = DbTree::new(db.open_tree(b"v2/tx_blobs")?);
        let tx_status = DbTree::new(db.open_tree(b"v2/tx_status")?);
        let tx_index = DbTree::new(db.open_tree(b"v2/tx_index")?);
        let tx_attempts = DbTree::new(db.open_tree(b"v2/tx_attempts")?);

        Ok(Self {
            db,
            envs,
            storage,
            agents,
            tx_auths,
            tx_blobs,
            tx_status,
            tx_index,
            tx_attempts,
        })
    }
}
