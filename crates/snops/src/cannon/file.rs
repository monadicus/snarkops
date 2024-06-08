use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Mutex,
};

use snops_common::state::TxPipeId;
use tracing::debug;

use super::error::CannonError;
use crate::cannon::error::TransactionSinkError;

#[derive(Debug)]
pub struct TransactionSink(Mutex<Option<BufWriter<File>>>);

impl TransactionSink {
    /// Create a new transaction sink
    pub fn new(storage_dir: PathBuf, target: TxPipeId) -> Result<Self, CannonError> {
        let target = storage_dir.join(target.to_string());
        debug!("opening tx sink @ {target:?}");

        let f = File::options()
            .create(true)
            .append(true)
            .open(&target)
            .map_err(|_| TransactionSinkError::FailedToOpenSource(target))?;

        Ok(Self(Mutex::new(Some(BufWriter::new(f)))))
    }

    /// Write a line to the transaction sink
    pub fn write(&self, line: &str) -> Result<(), CannonError> {
        let mut lock = self
            .0
            .lock()
            .map_err(|_| TransactionSinkError::FailedToLock)?;

        if lock.is_none() {
            return Ok(());
        }

        let writer = lock.as_mut().unwrap();
        writer
            .write_all(line.trim().as_bytes())
            .map_err(TransactionSinkError::FailedToWrite)?;
        writer
            .write_all(b"\n")
            .map_err(TransactionSinkError::FailedToWrite)?;
        writer
            .flush()
            .map_err(TransactionSinkError::FailedToWrite)?;
        Ok(())
    }
}
