use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    sync::{atomic::AtomicU32, Arc, Mutex},
};

use tracing::debug;

use super::error::CannonError;
use crate::{
    cannon::error::{TransactionDrainError, TransactionSinkError},
    schema::storage::{LoadedStorage, STORAGE_DIR},
    state::GlobalState,
};

#[derive(Debug)]
pub struct TransactionDrain {
    /// Line reader
    reader: Mutex<Option<BufReader<File>>>,
    pub(crate) line: AtomicU32,
}

impl TransactionDrain {
    /// Create a new transaction drain
    pub fn new_unread(
        state: &GlobalState,
        storage: Arc<LoadedStorage>,
        source: &str,
    ) -> Result<Self, CannonError> {
        Self::new(state, storage, source, 0)
    }
    /// Create a new transaction drain starting at a specific line
    pub fn new(
        state: &GlobalState,
        storage: Arc<LoadedStorage>,
        source: &str,
        line: u32,
    ) -> Result<Self, CannonError> {
        let source = state
            .cli
            .path
            .join(STORAGE_DIR)
            .join(storage.id.to_string())
            .join(source);
        debug!("opening tx drain @ {source:?}");

        let f = File::open(&source)
            .map_err(|e| TransactionDrainError::FailedToOpenSource(source, e))?;

        let mut reader = BufReader::new(f);

        // skip the first `line` lines
        let mut buf = String::new();
        for i in 0..line {
            // if the file is empty, return an empty drain
            if let Ok(0) = reader.read_line(&mut buf) {
                return Ok(Self {
                    reader: Mutex::new(None),
                    line: AtomicU32::new(i),
                });
            }
        }

        Ok(Self {
            reader: Mutex::new(Some(reader)),
            line: AtomicU32::new(line),
        })
    }

    /// Read the next line from the transaction drain
    pub fn next(&self) -> Result<Option<String>, CannonError> {
        let mut lock = self
            .reader
            .lock()
            .map_err(|_| TransactionDrainError::FailedToLock)?;

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
        self.line.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(Some(buf))
    }
}

#[derive(Debug)]
pub struct TransactionSink(Mutex<Option<BufWriter<File>>>);

impl TransactionSink {
    /// Create a new transaction sink
    pub fn new(
        state: &GlobalState,
        storage: Arc<LoadedStorage>,
        target: &str,
    ) -> Result<Self, CannonError> {
        let target = state
            .cli
            .path
            .join(STORAGE_DIR)
            .join(storage.id.to_string())
            .join(target);
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
