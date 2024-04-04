use std::{os::fd::AsRawFd, path::PathBuf};

use aleo_std::StorageMode;
use anyhow::{bail, ensure, Result};
use clap::Args;
use nix::{
    sys::wait::waitpid,
    unistd::{self, ForkResult},
};
use snarkvm::{
    ledger::Block,
    utilities::{FromBytes, ToBytes},
};
use tracing::info;

use crate::{checkpoint::Checkpoint, ledger::util, DbLedger, Network};

#[derive(Debug, Args)]
pub struct Truncate {
    #[arg(long)]
    height: Option<u32>,
    #[arg(long)]
    amount: Option<u32>,
    /// How many blocks to skip when reading
    #[arg(long, default_value_t = 1)]
    skip: u32,

    /// When replay mode is enabled, checkpoints are not used
    #[arg(long, default_value_t = false)]
    replay: bool,
    // TODO: duration based truncation (blocks within a duration before now)
    // TODO: timestamp based truncation (blocks after a certain date)
}

impl Truncate {
    pub fn parse(self, genesis: PathBuf, ledger: PathBuf) -> Result<()> {
        if self.replay {
            self.replay(genesis, ledger)
        } else {
            self.truncate(genesis, ledger)
        }
    }

    fn truncate(self, genesis: PathBuf, ledger_path: PathBuf) -> Result<()> {
        let genesis = Block::from_bytes_le(&std::fs::read(genesis)?)?;
        let storage_mode = StorageMode::Custom(ledger_path.clone());

        // open the ledger
        let ledger = DbLedger::load(genesis.clone(), storage_mode.clone())?;

        let height = match (self.height, self.amount) {
            (Some(height), _) => height,
            (None, Some(amount)) => ledger.latest_height().saturating_sub(amount),
            // Clap should prevent this case
            _ => bail!("Either height or amount must be specified"),
        };

        // write the checkpoint file
        info!("loading checkpoint...");
        let path = ledger_path
            .parent()
            .unwrap()
            .join(format!("{height}.checkpoint"));

        ensure!(path.exists(), "checkpoint file does not exist");

        let bytes = std::fs::read(path)?;
        let checkpoint = Checkpoint::<Network>::from_bytes_le(&bytes)?;
        info!("read checkpoint for height {}", checkpoint.height());

        info!("applying checkpoint to ledger...");
        checkpoint.rewind(&ledger, storage_mode.clone())?;
        info!("successfully applied checkpoint");
        Ok(())
    }

    fn replay(self, genesis: PathBuf, mut ledger: PathBuf) -> Result<()> {
        let (read_fd, write_fd) = unistd::pipe()?;

        match unsafe { unistd::fork() }? {
            ForkResult::Parent { child, .. } => {
                unistd::close(read_fd.as_raw_fd())?;

                let db_ledger: DbLedger = util::open_ledger(genesis, ledger)?;

                let amount = match (self.height, self.amount) {
                    (Some(height), _) => db_ledger.latest_height() - height,
                    (None, Some(amount)) => amount,
                    // Clap should prevent this case
                    _ => bail!("Either height or amount must be specified"),
                };

                let target_height = db_ledger.latest_height().saturating_sub(amount);

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
                let db_ledger: DbLedger = util::open_ledger(genesis, ledger)?;
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
