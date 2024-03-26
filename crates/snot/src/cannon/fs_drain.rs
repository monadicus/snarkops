use std::{
    fs::File,
    io::{BufRead, BufReader},
    sync::{Arc, Mutex},
};

use anyhow::{bail, Result};

use crate::schema::storage::LoadedStorage;

#[derive(Debug)]
pub struct TransactionDrain(Mutex<Option<BufReader<File>>>);

impl TransactionDrain {
    /// Create a new transaction drain
    pub fn new(storage: Arc<LoadedStorage>, source: &str) -> Result<Self> {
        let source = storage.path.join(source);

        let Ok(f) = File::open(&source) else {
            bail!("error opening transaction source file: {source:?}");
        };

        Ok(Self(Mutex::new(Some(BufReader::new(f)))))
    }

    /// Read the next line from the transaction drain
    pub fn next(&self) -> Result<Option<String>> {
        let Ok(mut lock) = self.0.lock() else {
            bail!("error locking transaction drain");
        };

        if lock.is_none() {
            return Ok(None);
        }

        let mut buf = String::new();
        // read a line and clear the lock on EOF
        if lock.as_mut().unwrap().read_line(&mut buf)? == 0 {
            *lock = None;
            return Ok(None);
        }
        Ok(Some(buf))
    }
}
