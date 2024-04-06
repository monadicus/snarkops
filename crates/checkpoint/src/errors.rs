use std::{io, path::PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManagerLoadError {
    #[error("invalid storage path: {0}")]
    InvalidStoragePath(PathBuf),
    #[error("error globbing storage path: {0}")]
    GlobError(#[from] glob::PatternError),
}

#[derive(Debug, Error)]
#[cfg(feature = "write")]
pub enum ManagerCullError {
    #[error("error opening storage: {0}")]
    StorageOpenError(#[source] anyhow::Error),
    #[error("error reading ledger: {0}")]
    ReadLedger(#[source] anyhow::Error),
}

#[derive(Debug, Error)]
#[cfg(feature = "write")]
pub enum ManagerPollError {
    #[error("error reading checkpoint header: {0}")]
    Header(#[from] CheckpointHeaderError),
    #[error("error reading checkpoint: {0}")]
    Read(#[from] CheckpointReadError),
    #[error("error inserting checkpoint: {0}")]
    Insert(#[from] ManagerInsertError),
}

#[derive(Debug, Error)]
#[cfg(feature = "write")]
pub enum ManagerInsertError {
    #[error("invalid storage path: {0}")]
    InvalidStoragePath(PathBuf),
    #[error("error opening file: {0}")]
    FileError(#[source] io::Error),
    #[error("error modifying file times: {0}")]
    ModifyError(#[source] io::Error),
    #[error("error writing file: {0}")]
    WriteError(#[source] io::Error),
}

#[derive(Debug, Error)]
#[cfg(feature = "write")]
pub enum CheckpointReadError {
    #[error("error reading checkpoint header: {0}")]
    Header(#[from] CheckpointHeaderError),
    #[error("error reading checkpoint content: {0}")]
    Content(#[from] CheckpointContentError),
}

#[derive(Debug, Error)]
#[cfg(feature = "write")]
pub enum CheckpointCheckError {
    #[error("error opening storage: {0}")]
    StorageOpenError(#[source] anyhow::Error),
    #[error("block hash not found at height {0}")]
    BlockNotFound(u32),
    #[error("error reading ledger: {0}")]
    ReadLedger(#[source] anyhow::Error),
    #[error("checkpoint height ({0}) is greater than ledger height ({1})")]
    HeightMismatch(u32, u32),
    #[error("checkpoint hash ({1}) does not match ledger hash ({2}) at height {0}")]
    HashMismatch(u32, String, String),
}

#[derive(Debug, Error)]
#[cfg(feature = "write")]
pub enum CheckpointRewindError {
    #[error("error opening storage: {0}")]
    OpenLedger(#[source] anyhow::Error),
    #[error("error reading ledger: {0}")]
    ReadLedger(#[source] anyhow::Error),
    #[error("error checking checkpoint: {0}")]
    Check(#[from] CheckpointCheckError),
    #[error("error removing documents: {0}")]
    RemoveDocument(#[source] anyhow::Error),
    #[error("no commiteee found for height {0}. ledger likely corrupted")]
    MissingCommittee(u32),
}

#[derive(Debug, Error)]
pub enum CheckpointHeaderError {
    #[error("error opening file: {0}")]
    FileError(#[source] io::Error),
    #[error("error reading bytes: {0}")]
    ReadError(#[source] io::Error),
    #[error("no block found at height {0}")]
    BlockNotFound(u32),
    #[error("no genesis block hash found in storage")]
    HashlessGenesis,
    #[error("no block header found for block hash {1} at height {0}")]
    BlockMissingHeader(u32, String),

    #[error("error opening storage: {0}")]
    #[cfg(feature = "write")]
    OpenLedger(#[source] anyhow::Error),
    #[error("error reading ledger: {0}")]
    #[cfg(feature = "write")]
    ReadLedger(#[source] anyhow::Error),
}

#[derive(Debug, Error)]
#[cfg(feature = "write")]
pub enum CheckpointContentError {
    #[error("error opening storage: {0}")]
    OpenLedger(#[source] anyhow::Error),
    #[error("error reading ledger: {0}")]
    ReadLedger(#[source] anyhow::Error),
    #[error("no block found at height {0}")]
    BlockNotFound(u32),
    #[error("no genesis block hash found in storage")]
    HashlessGenesis,
    #[error("no block header found for block hash {1} at height {0}")]
    BlockMissingHeader(u32, String),
}
