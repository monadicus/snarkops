use std::{os::fd::AsRawFd, path::PathBuf};

use aleo_std::StorageMode;
use anyhow::{bail, ensure, Result};
use checkpoint::{Checkpoint, CheckpointManager, RetentionPolicy};
use clap::{Args, Parser};
use nix::{
    sys::wait::waitpid,
    unistd::{self, ForkResult},
};
use snarkvm::{
    console::program::Network,
    ledger::Block,
    utilities::{FromBytes, ToBytes},
};
use tracing::info;

use crate::{ledger::util, DbLedger};

/// Replays blocks from a ledger to a specific height or amount to rollback to.
#[derive(Debug, Args)]
pub struct Replay {
    /// The height to replay to.
    #[arg(long)]
    height: Option<u32>,
    /// The amount of blocks to rollback to.
    #[arg(long)]
    amount: Option<u32>,
    /// How many blocks to skip when reading.
    #[arg(long, default_value_t = 1)]
    skip: u32,
    /// When checkpoint is enabled, checkpoints.
    #[arg(short, long, default_value_t = false)]
    checkpoint: bool,
    // TODO: duration based truncation (blocks within a duration before now)
    // TODO: timestamp based truncation (blocks after a certain date)
}

/// A command to truncate the ledger to a specific height.
#[derive(Debug, Parser)]
pub enum Truncate {
    /// Rewind the ledger to a specific checkpoint.
    Rewind {
        /// The checkpoint to rewind to.
        checkpoint: PathBuf,
    },
    Replay(Replay),
}

impl Truncate {
    pub fn parse<N: Network>(self, genesis: Block<N>, ledger: PathBuf) -> Result<()> {
        match self {
            Truncate::Rewind { checkpoint } => Self::rewind::<N>(genesis, ledger, checkpoint),
            Truncate::Replay(replay) => replay.parse::<N>(genesis, ledger),
        }
    }

    pub fn rewind<N: Network>(
        genesis: Block<N>,
        ledger_path: PathBuf,
        checkpoint_path: PathBuf,
    ) -> Result<()> {
        let storage_mode = StorageMode::Custom(ledger_path.clone());

        // open the ledger
        let ledger = DbLedger::<N>::load(genesis.clone(), storage_mode.clone())?;

        ensure!(checkpoint_path.exists(), "checkpoint file does not exist");

        let bytes = std::fs::read(checkpoint_path)?;
        let checkpoint = Checkpoint::from_bytes_le(&bytes)?;
        info!("read checkpoint for height {}", checkpoint.height());

        info!("applying checkpoint to ledger...");
        checkpoint.rewind(&ledger, storage_mode.clone())?;
        info!("successfully applied checkpoint");
        Ok(())
    }
}

impl Replay {
    fn parse<N: Network>(self, genesis: Block<N>, mut ledger: PathBuf) -> Result<()> {
        let (read_fd, write_fd) = unistd::pipe()?;

        match unsafe { unistd::fork() }? {
            ForkResult::Parent { child, .. } => {
                unistd::close(read_fd.as_raw_fd())?;

                let db_ledger: DbLedger<N> = util::open_ledger(genesis, ledger)?;

                let target_height = match (self.height, self.amount) {
                    (Some(height), _) => height,
                    (None, Some(amount)) => db_ledger.latest_height().saturating_sub(amount),
                    // Clap should prevent this case
                    _ => bail!("Either height or amount must be specified"),
                };

                for i in self.skip..target_height {
                    let block = db_ledger.get_block(i)?;
                    let buf = block.to_bytes_le()?;
                    // println!("Writing block {i}... {}", buf.len());

                    unistd::write(&write_fd, &(buf.len() as u32).to_le_bytes())?;
                    unistd::write(&write_fd, &buf)?;
                }

                unistd::write(&write_fd, &(0u32).to_le_bytes())?;
                unistd::close(write_fd.as_raw_fd())?;

                waitpid(child, None).unwrap();
            }
            ForkResult::Child => {
                unistd::close(write_fd.as_raw_fd())?;

                ledger.set_extension("new");

                let mut manager = self
                    .checkpoint
                    .then(|| CheckpointManager::load(ledger.clone(), RetentionPolicy::default()))
                    .transpose()?;

                let db_ledger: DbLedger<N> = util::open_ledger(genesis, ledger)?;

                // wipe out existing incompatible checkpoints
                if let Some(manager) = manager.as_mut() {
                    manager.cull_incompatible::<N>()?;
                }

                let read_fd = read_fd.as_raw_fd();

                loop {
                    let mut size_buf = [0u8; 4];
                    unistd::read(read_fd, &mut size_buf)?;
                    let amount = u32::from_le_bytes(size_buf);
                    if amount == 0 {
                        break;
                    }

                    let mut buf = vec![0u8; amount as usize];
                    let mut read = 0;
                    while read < amount as usize {
                        read += unistd::read(read_fd, &mut buf[read..])?;
                    }
                    let block = Block::from_bytes_le(&buf)?;
                    if db_ledger.latest_height() + 1 != block.height() {
                        println!(
                            "Skipping block {}, waiting for {}",
                            block.height(),
                            db_ledger.latest_height() + 1,
                        );
                    } else {
                        println!(
                            "Reading block {}... {}",
                            db_ledger.latest_height() + 1,
                            buf.len()
                        );

                        db_ledger.advance_to_next_block(&block)?;

                        // if checkpoints are enabled, check if this block should be added
                        if let Some(manager) = manager.as_mut() {
                            manager.poll::<N>()?;
                        }
                    }
                }

                unistd::close(read_fd.as_raw_fd())?;
                unsafe {
                    nix::libc::_exit(0);
                }
            }
        }

        Ok(())
    }
}
