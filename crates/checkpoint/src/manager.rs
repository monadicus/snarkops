use std::{collections::BTreeMap, fs, path::PathBuf};

use chrono::{DateTime, TimeDelta, Utc};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tracing::{error, trace};

#[cfg(feature = "write")]
use crate::errors::{ManagerCullError, ManagerInsertError, ManagerPollError};
use crate::{errors::ManagerLoadError, path_from_height, CheckpointHeader, RetentionPolicy};

#[derive(Debug, Clone)]
pub struct CheckpointManager {
    #[cfg(feature = "write")]
    storage_path: PathBuf,
    policy: RetentionPolicy,
    /// timestamp -> checkpoint header
    checkpoints: BTreeMap<DateTime<Utc>, (CheckpointHeader, PathBuf)>,
}

/// Block timestamps are seconds since Unix epoch UTC
fn datetime_from_int(timestamp: i64) -> DateTime<Utc> {
    DateTime::UNIX_EPOCH + TimeDelta::new(timestamp, 0).unwrap()
}

impl CheckpointManager {
    /// Given a storage path, load headers from the available checkpoints into
    /// memory
    pub fn load(storage_path: PathBuf, policy: RetentionPolicy) -> Result<Self, ManagerLoadError> {
        use ManagerLoadError::*;

        // assemble glob checkpoint files based on the provided storage path
        let Some(storage_glob) = path_from_height(&storage_path, "*") else {
            return Err(InvalidStoragePath(storage_path));
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
                let header = match CheckpointHeader::read_file(&path) {
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
            #[cfg(feature = "write")]
            storage_path,
            checkpoints,
            policy,
        })
    }

    /// Cull checkpoints that are incompatible with the current block database
    #[cfg(feature = "write")]
    pub fn cull_incompatible<N: crate::aleo::Network>(
        &mut self,
    ) -> Result<usize, ManagerCullError> {
        use ManagerCullError::*;

        use crate::aleo::*;

        let blocks = BlockDB::<N>::open(StorageMode::Custom(self.storage_path.clone()))
            .map_err(StorageOpenError)?;

        let mut rejected = vec![];

        for (time, (header, path)) in self.checkpoints.iter() {
            let height = header.block_height;
            let Some(block_hash): Option<BlockHash<N>> =
                blocks.get_block_hash(height).map_err(ReadLedger)?
            else {
                trace!("checkpoint {path:?} at height {height} is taller than the ledger");
                rejected.push(*time);
                continue;
            };
            if block_bytes::<N>(&block_hash) != header.block_hash {
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

    /// Delete all checkpoints stored by this manager
    pub fn wipe(&mut self) {
        for (_header, path) in self.checkpoints.values() {
            if let Err(err) = fs::remove_file(path) {
                error!("error deleting checkpoint {path:?}: {err}");
            }
        }
        self.checkpoints.clear();
    }

    /// Poll the ledger for a new checkpoint and write it to disk
    /// Also reject old checkpoints that are no longer needed
    #[cfg(feature = "write")]
    pub fn poll<N: crate::aleo::Network>(&mut self) -> Result<bool, ManagerPollError> {
        let header = CheckpointHeader::read_ledger::<N>(self.storage_path.clone())?;
        let time = header.time();

        if !self.is_ready(&time) || header.block_height == 0 {
            return Ok(false);
        }

        trace!("creating checkpoint @ {}...", header.block_height);
        let checkpoint =
            crate::Checkpoint::<N>::new_from_header(self.storage_path.clone(), header)?;
        self.write_and_insert(checkpoint)?;
        self.cull_timestamp(time);
        Ok(true)
    }

    /// Check if the manager is ready to create a new checkpoint given the
    /// current timestamp
    pub fn is_ready(&self, timestamp: &DateTime<Utc>) -> bool {
        let Some((last_time, _)) = self.checkpoints.last_key_value() else {
            // if this is the first checkpoint, it is ready
            return true;
        };

        self.policy.is_ready_with_time(timestamp, last_time)
    }

    /// Write a checkpoint to disk and insert it into the manager
    #[cfg(feature = "write")]
    pub fn write_and_insert<N: crate::aleo::Network>(
        &mut self,
        checkpoint: crate::Checkpoint<N>,
    ) -> Result<(), ManagerInsertError> {
        use ManagerInsertError::*;

        use crate::aleo::ToBytes;

        let Some(path) = path_from_height(&self.storage_path, checkpoint.height()) else {
            return Err(InvalidStoragePath(self.storage_path.clone()));
        };

        // write to a file
        let mut writer = fs::File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .map_err(FileError)?;
        writer
            .set_times(std::fs::FileTimes::new().set_modified(checkpoint.header.time().into()))
            .map_err(ModifyError)?;
        checkpoint.write_le(&mut writer).map_err(WriteError)?;

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
        let times = self.checkpoints.keys().collect();
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

    /// Get the retention policy used by this manager
    pub fn policy(&self) -> &RetentionPolicy {
        &self.policy
    }

    /// Iterate the checkpoints stored by this manager
    pub fn checkpoints(&self) -> impl Iterator<Item = &(CheckpointHeader, PathBuf)> {
        self.checkpoints.values()
    }
}

impl std::fmt::Display for CheckpointManager {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut prev_time: Option<DateTime<Utc>> = None;
        write!(f, "{} checkpoints:", self.checkpoints.len())?;
        for (time, (header, _)) in &self.checkpoints {
            write!(
                f,
                "\n  {time}: block {}, {}",
                header.block_height,
                if let Some(prev) = prev_time {
                    format!(
                        "{}hr later",
                        time.signed_duration_since(prev).num_seconds() / 3600
                    )
                } else {
                    "".to_string()
                }
            )?;
            prev_time = Some(*time);
        }
        Ok(())
    }
}
