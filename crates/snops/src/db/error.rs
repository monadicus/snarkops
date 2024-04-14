#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("opening db: {0}")]
    Open(#[from] sled::Error),
    #[error("looking up key {0} in {1}: {2}")]
    LookupError(String, String, sled::Error),
    #[error("deleting key {0} in {1}: {2}")]
    DeleteError(String, String, sled::Error),
    #[error("save error key {0} in {1}: {2}")]
    SaveError(String, String, sled::Error),
    #[error("deserialize value {0} in {1}: {2}")]
    DeserializeError(String, String, bincode::Error),
    #[error("serialize value {0} in {1}: {2}")]
    SerializeError(String, String, bincode::Error),
    #[error("missing key {0} in {1}")]
    MissingKey(String, String),
    #[error("unknown document version {2} for: {1} in {0}")]
    UnsupportedVersion(String, String, u8),
    #[error("transaction error: {0}")]
    TransactionError(#[from] sled::transaction::TransactionError),
}
