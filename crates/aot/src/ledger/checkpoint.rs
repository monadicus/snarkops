use crate::{ledger::util, runner::checkpoint::Checkpoint, DbLedger, Network};
use aleo_std::StorageMode;
use anyhow::Result;
use snarkvm::utilities::ToBytes;
use std::path::PathBuf;
use tracing::{info, trace};

pub fn open_and_checkpoint(genesis: PathBuf, ledger: PathBuf) -> Result<()> {
    let storage_mode = StorageMode::Custom(ledger.clone());
    let ledger: DbLedger = util::open_ledger(genesis, ledger)?;
    let height = ledger.latest_height();

    info!("creating checkpoint @ {height}...");
    let bytes = Checkpoint::<Network>::new(height, storage_mode.clone())?.to_bytes_le()?;

    info!("created checkpoint; {} bytes", bytes.len());

    if let StorageMode::Custom(path) = storage_mode {
        // write the checkpoint file
        let path = path.parent().unwrap().join(format!("{height}.checkpoint"));
        std::fs::write(&path, bytes)?;
        trace!("checkpoint written to {path:?}");
    };

    Ok(())
}
