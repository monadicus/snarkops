use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
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

#[derive(Debug)]
pub struct TransactionSink(Mutex<Option<BufWriter<File>>>);

impl TransactionSink {
    /// Create a new transaction sink
    pub fn new(storage: Arc<LoadedStorage>, target: &str) -> Result<Self> {
        let target = storage.path.join(target);

        let Ok(f) = File::options().create(true).append(true).open(&target) else {
            bail!("error opening transaction target file: {target:?}");
        };

        Ok(Self(Mutex::new(Some(BufWriter::new(f)))))
    }

    /// Write a line to the transaction sink
    pub fn write(&self, line: &str) -> Result<()> {
        let Ok(mut lock) = self.0.lock() else {
            bail!("error locking transaction sink");
        };

        if lock.is_none() {
            return Ok(());
        }

        let writer = lock.as_mut().unwrap();
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
        Ok(())
    }
}
