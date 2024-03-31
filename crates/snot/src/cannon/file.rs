use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    sync::{Arc, Mutex},
};

use tracing::debug;

use super::error::CannonError;
use crate::{
    cannon::error::{TransactionDrainError, TransactionSinkError},
    schema::storage::LoadedStorage,
};

#[derive(Debug)]
pub struct TransactionDrain(Mutex<Option<BufReader<File>>>);

impl TransactionDrain {
    /// Create a new transaction drain
    pub fn new(storage: Arc<LoadedStorage>, source: &str) -> Result<Self, CannonError> {
        let source = storage.path.join(source);
        debug!("opening tx drain @ {source:?}");

        let Ok(f) = File::open(&source) else {
            return Err(TransactionDrainError::FailedToOpenSource(source).into());
        };

        Ok(Self(Mutex::new(Some(BufReader::new(f)))))
    }

    /// Read the next line from the transaction drain
    pub fn next(&self) -> Result<Option<String>, CannonError> {
        let Ok(mut lock) = self.0.lock() else {
            return Err(TransactionDrainError::FailedToLock.into());
        };

        if lock.is_none() {
            return Ok(None);
        }

        let mut buf = String::new();
        // read a line and clear the lock on EOF
        if lock
            .as_mut()
            .unwrap()
            .read_line(&mut buf)
            .map_err(TransactionDrainError::FailedToReadLine)?
            == 0
        {
            *lock = None;
            return Ok(None);
        }
        Ok(Some(buf))
    }
}

#[derive(Debug)]
pub struct TransactionSink(Mutex<Option<BufWriter<File>>>);

impl TransactionSink {
    /// Create a new transaction sink
    pub fn new(storage: Arc<LoadedStorage>, target: &str) -> Result<Self, CannonError> {
        let target = storage.path.join(target);
        debug!("opening tx sink @ {target:?}");

        let Ok(f) = File::options().create(true).append(true).open(&target) else {
            return Err(TransactionSinkError::FailedToOpenSource(target).into());
        };

        Ok(Self(Mutex::new(Some(BufWriter::new(f)))))
    }

    /// Write a line to the transaction sink
    pub fn write(&self, line: &str) -> Result<(), CannonError> {
        let Ok(mut lock) = self.0.lock() else {
            return Err(TransactionSinkError::FailedToLock.into());
        };

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
