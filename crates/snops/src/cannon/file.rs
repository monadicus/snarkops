use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
    sync::{atomic::AtomicU32, Mutex},
};

use snops_common::state::TxPipeId;
use tracing::debug;

use super::{error::CannonError, ExecutionContext};
use crate::{
    cannon::error::{TransactionDrainError, TransactionSinkError},
    db::document::DbDocument,
    env::persist::PersistDrainCount,
};

#[derive(Debug)]
pub struct TransactionDrain {
    id: TxPipeId,
    /// Line reader
    reader: Mutex<Option<BufReader<File>>>,
    pub(crate) line: AtomicU32,
}

impl TransactionDrain {
    /// Create a new transaction drain
    pub fn new_unread(storage_path: PathBuf, source: TxPipeId) -> Result<Self, CannonError> {
        Self::new(storage_path, source, 0)
    }
    /// Create a new transaction drain starting at a specific line
    pub fn new(storage_path: PathBuf, id: TxPipeId, line: u32) -> Result<Self, CannonError> {
        let source = storage_path.join(id.to_string());
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
                    id,
                    reader: Mutex::new(None),
                    line: AtomicU32::new(i),
                });
            }
        }

        Ok(Self {
            id,
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

    /// Save persistence for this drain
    pub fn write_persistence(&self, ctx: &ExecutionContext) {
        let Some(env) = ctx.env.upgrade() else {
            return;
        };

        let key = (env.id, self.id);
        let count = self.line.load(std::sync::atomic::Ordering::Relaxed);

        if let Err(e) = (PersistDrainCount { count }).save(&ctx.state.db, key) {
            tracing::error!(
                "Error saving drain count for env {}, drain {}: {e}",
                env.id,
                self.id
            );
        }
    }
}

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
