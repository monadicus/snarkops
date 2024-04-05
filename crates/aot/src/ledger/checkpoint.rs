use crate::{
    checkpoint::{path_from_height, Checkpoint},
    ledger::util,
    DbLedger, Network,
};
use anyhow::Result;
use snarkvm::utilities::ToBytes;
use std::path::PathBuf;
use tracing::{info, trace};

pub fn open_and_checkpoint(genesis: PathBuf, ledger_path: PathBuf) -> Result<()> {
    let ledger: DbLedger = util::open_ledger(genesis, ledger_path.clone())?;
    let height = ledger.latest_height();

    info!("creating checkpoint @ {height}...");
    let bytes = Checkpoint::<Network>::new(ledger_path.clone())?.to_bytes_le()?;

    info!("created checkpoint; {} bytes", bytes.len());

    if let Some(path) = path_from_height(&ledger_path, height) {
        // write the checkpoint file
        std::fs::write(&path, bytes)?;
        trace!("checkpoint written to {path:?}");
    };

    Ok(())
}
