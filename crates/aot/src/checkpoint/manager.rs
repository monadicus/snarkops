use aleo_std::StorageMode;
use anyhow::{bail, Result};
use chrono::{DateTime, TimeDelta, Utc};
use rayon::iter::ParallelIterator;
use snarkvm::{
    console::program::{Itertools, Network},
    ledger::store::{helpers::rocksdb::BlockDB, BlockStorage},
    utilities::ToBytes,
};
use std::{collections::BTreeMap, fs, path::PathBuf};
use tracing::{error, trace};

use crate::checkpoint::path_from_height;

use super::{Checkpoint, CheckpointHeader, RetentionPolicy};

pub struct CheckpointManager<N: Network> {
    storage_path: PathBuf,
    policy: RetentionPolicy,
    /// timestamp -> checkpoint header
    checkpoints: BTreeMap<DateTime<Utc>, (CheckpointHeader<N>, PathBuf)>,
}

/// Block timestamps are seconds since Unix epoch UTC
fn datetime_from_int(timestamp: i64) -> DateTime<Utc> {
    DateTime::UNIX_EPOCH + TimeDelta::new(timestamp, 0).unwrap()
}

impl<N: Network> CheckpointManager<N> {
    /// Given a storage path, load headers from the available checkpoints into memory
    pub fn load(storage_path: PathBuf, policy: RetentionPolicy) -> Result<Self> {
        use rayon::iter::IntoParallelIterator;

        // assemble glob checkpoint files based on the provided storage path
        let Some(storage_glob) = path_from_height(&storage_path, "*") else {
            bail!("invalid storage path");
        };
        let paths = glob::glob(&storage_glob.to_string_lossy())?;
        // this ridiculous Path result from glob is NOT IntoParallelIterator
        let paths = paths.into_iter().collect::<Vec<_>>();

        // read checkpoint headers in parallel
        let checkpoints = paths
            .into_par_iter()
            .filter_map(|path| {
                let path = match path {
                    Ok(path) => path,
                    Err(err) => {
                        error!("error globbing {storage_path:?}: {err}");
                        return None;
                    }
                };

                // parse only the header from the given path
                let header = match CheckpointHeader::<N>::read_file(&path) {
                    Ok(header) => header,
                    Err(err) => {
                        error!("error parsing {path:?}: {err}");
                        return None;
                    }
                };

                let timestamp = datetime_from_int(header.timestamp);
                Some((timestamp, (header, path)))
            })
            .collect();

        Ok(Self {
            storage_path,
            checkpoints,
            policy,
        })
    }

    /// Cull checkpoints that are incompatible with the current block database
    pub fn cull_incompatible(&mut self) -> Result<usize> {
        let blocks = BlockDB::<N>::open(StorageMode::Custom(self.storage_path.clone()))?;

        let mut rejected = vec![];

        for (time, (header, path)) in self.checkpoints.iter() {
            let height = header.block_height;
            let block_hash = blocks.get_block_hash(height)?;
            if block_hash != Some(header.block_hash) {
                trace!("checkpoint {path:?} is incompatible with block at height {height}");
                rejected.push(*time);
            }
        }

        let count = rejected.len();
        for time in rejected {
            if let Some((_header, path)) = self.checkpoints.remove(&time) {
                if let Err(err) = fs::remove_file(&path) {
                    error!("error deleting incompatible checkpoint {path:?}: {err}");
                }
            }
        }

        Ok(count)
    }

    /// Poll the ledger for a new checkpoint and write it to disk
    /// Also reject old checkpoints that are no longer needed
    pub fn poll(&mut self) -> Result<bool> {
        let header = CheckpointHeader::<N>::read_ledger(self.storage_path.clone())?;
        let time = header.time();

        if !self.is_ready(&time) || header.block_height == 0 {
            return Ok(false);
        }

        let checkpoint = Checkpoint::<N>::new_from_header(self.storage_path.clone(), header)?;
        self.write_and_insert(checkpoint)?;
        self.cull_timestamp(time);
        Ok(true)
    }

    /// Check if the manager is ready to create a new checkpoint given the current timestamp
    pub fn is_ready(&self, timestamp: &DateTime<Utc>) -> bool {
        let Some((last_time, _)) = self.checkpoints.last_key_value() else {
            // if this is the first checkpoint, it is ready
            return true;
        };

        self.policy.is_ready_with_time(timestamp, last_time)
    }

    /// Write a checkpoint to disk and insert it into the manager
    pub fn write_and_insert(&mut self, checkpoint: Checkpoint<N>) -> Result<()> {
        let Some(path) = path_from_height(&self.storage_path, checkpoint.height()) else {
            bail!("invalid storage path");
        };

        // write to a file
        let mut writer = fs::File::options().write(true).create(true).open(&path)?;
        checkpoint.write_le(&mut writer)?;

        trace!(
            "checkpoint on {} @ {} written to {path:?}",
            checkpoint.header.time(),
            checkpoint.height(),
        );

        self.checkpoints
            .insert(checkpoint.header.time(), (checkpoint.header, path));
        Ok(())
    }

    pub fn cull(&mut self) {
        self.cull_timestamp(Utc::now());
    }

    /// Remove the oldest checkpoints that are no longer needed
    pub fn cull_timestamp(&mut self, timestamp: DateTime<Utc>) {
        let times = self.checkpoints.keys().collect_vec();
        let rejected = self.policy.reject_with_time(timestamp, times);
        for time in rejected {
            if let Some((_header, path)) = self.checkpoints.remove(&time) {
                trace!("deleting rejected checkpoint {path:?}");
                if let Err(err) = fs::remove_file(&path) {
                    error!("error deleting rejected checkpoint {path:?}: {err}");
                }
            }
        }
    }
}
