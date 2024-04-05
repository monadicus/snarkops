use std::{collections::BTreeMap, path::PathBuf};

use aleo_std::StorageMode;
use anyhow::{bail, Result};
use chrono::{DateTime, TimeDelta, Utc};
use rayon::iter::ParallelIterator;
use snarkvm::console::program::Network;
use tracing::{error, warn};

use super::{path_from_storage, CheckpointHeader};

pub struct CheckpointManager<N: Network> {
    storage_path: PathBuf,
    /// timestamp -> checkpoint header
    checkpoints: BTreeMap<DateTime<Utc>, (CheckpointHeader<N>, PathBuf)>,
}

/// Block timestamps are seconds since Unix epoch UTC
fn instant_from_timestamp(timestamp: i64) -> DateTime<Utc> {
    DateTime::UNIX_EPOCH + TimeDelta::new(timestamp, 0).unwrap()
}

impl<N: Network> CheckpointManager<N> {
    /// Given a storage path, load headers from the available checkpoints into memory
    pub fn load(storage_path: PathBuf) -> Result<Self> {
        use rayon::iter::IntoParallelIterator;

        // assemble glob checkpoint files based on the provided storage path
        let Some(storage_glob) = path_from_storage(&StorageMode::Custom(storage_path.clone()), "*")
        else {
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

                let timestamp = instant_from_timestamp(header.timestamp);
                Some((timestamp, (header, path)))
            })
            .collect();

        Ok(Self {
            storage_path,
            checkpoints,
        })
    }
}
