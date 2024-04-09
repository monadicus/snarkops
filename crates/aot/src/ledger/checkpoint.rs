use std::path::PathBuf;

use anyhow::Result;
use checkpoint::{path_from_height, Checkpoint, CheckpointManager, RetentionPolicy};
use clap::Parser;
use snarkvm::utilities::ToBytes;
use tracing::{info, trace};

use super::truncate::Truncate;
use crate::{ledger::util, DbLedger};

#[derive(Debug, Parser)]
pub enum CheckpointCommand {
    /// Create a checkpoint for the given ledger
    Create,
    /// Apply a checkpoint to the given ledger
    Apply {
        /// Checkpoint file to apply
        checkpoint: PathBuf,
        /// When present, clean up old checkpoints that are no longer applicable
        /// after applying the checkpoint
        #[clap(long, short, default_value = "false")]
        clean: bool,
    },
    /// View the available checkpoints
    View,
    /// Cleanup old checkpoints
    Clean,
}

impl CheckpointCommand {
    pub fn parse(self, genesis: PathBuf, ledger: PathBuf) -> Result<()> {
        match self {
            CheckpointCommand::Create => open_and_checkpoint(genesis, ledger),
            CheckpointCommand::Apply { checkpoint, clean } => {
                Truncate::rewind(genesis, ledger.clone(), checkpoint)?;
                if clean {
                    let mut manager = CheckpointManager::load(ledger, RetentionPolicy::default())?;
                    info!("removed {} old checkpoints", manager.cull_incompatible()?);
                }
                Ok(())
            }
            CheckpointCommand::View => {
                let manager = CheckpointManager::load(ledger, RetentionPolicy::default())?;
                println!("{manager}");
                Ok(())
            }
            CheckpointCommand::Clean => {
                let mut manager = CheckpointManager::load(ledger, RetentionPolicy::default())?;
                info!("removed {} old checkpoints", manager.cull_incompatible()?);
                Ok(())
            }
        }
    }
}

pub fn open_and_checkpoint(genesis: PathBuf, ledger_path: PathBuf) -> Result<()> {
    let ledger: DbLedger = util::open_ledger(genesis, ledger_path.clone())?;
    let height = ledger.latest_height();

    info!("creating checkpoint @ {height}...");
    let bytes = Checkpoint::new(ledger_path.clone())?.to_bytes_le()?;

    info!("created checkpoint; {} bytes", bytes.len());

    if let Some(path) = path_from_height(&ledger_path, height) {
        // write the checkpoint file
        std::fs::write(&path, bytes)?;
        trace!("checkpoint written to {path:?}");
    };

    Ok(())
}
