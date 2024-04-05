use std::{
    fmt::Display,
    path::{Path, PathBuf},
    sync::Arc,
};

use aleo_std::StorageMode;
use anyhow::{bail, ensure, Result};
use snarkos_node::bft::{
    helpers::Storage, ledger_service::CoreLedgerService, storage_service::BFTMemoryService,
};
use snarkvm::{
    console::program::Network as NetworkTrait,
    ledger::{
        store::{
            helpers::{
                rocksdb::{BlockDB, CommitteeDB, ConsensusDB, FinalizeDB},
                Map, MapRead,
            },
            BlockStorage, CommitteeStorage, FinalizeStorage,
        },
        Ledger,
    },
};

mod content;
mod header;
mod manager;
mod retention;

pub use content::*;
pub use header::*;
pub use manager::*;
pub use retention::*;

pub fn path_from_storage<D: Display>(mode: &StorageMode, height: D) -> Option<PathBuf> {
    match mode {
        StorageMode::Custom(path) => path_from_height(path, height),
        _ => None,
    }
}

pub fn path_from_height<D: Display>(path: &Path, height: D) -> Option<PathBuf> {
    path.parent()
        .map(|p| p.join(format!("{height}.checkpoint")))
}

impl<N: NetworkTrait> Checkpoint<N> {
    pub fn new_from_header(path: PathBuf, header: CheckpointHeader<N>) -> Result<Self> {
        let content = CheckpointContent::read_ledger(path)?;
        Ok(Self { header, content })
    }

    pub fn new(path: PathBuf) -> Result<Self> {
        let header = CheckpointHeader::read_ledger(path.clone())?;
        let content = CheckpointContent::read_ledger(path)?;

        Ok(Self { header, content })
    }

    pub fn check(&self, storage_mode: StorageMode) -> Result<()> {
        let blocks = BlockDB::<N>::open(storage_mode.clone())?;
        let committee = CommitteeDB::<N>::open(storage_mode.clone())?;
        let height = committee.current_height()?;

        ensure!(
            height > self.height(),
            "checkpoint is for a height greater than the current height"
        );
        ensure!(
            blocks.get_block_hash(self.height())? == Some(self.header.block_hash),
            "checkpoint block hash does not appear to belong to the block at the checkpoint height"
        );

        Ok(())
    }

    pub fn rewind(
        self,
        ledger: &Ledger<N, ConsensusDB<N>>,
        storage_mode: StorageMode,
    ) -> Result<()> {
        let finalize = FinalizeDB::<N>::open(storage_mode.clone())?;
        let blocks = BlockDB::<N>::open(storage_mode.clone())?;
        let committee = CommitteeDB::<N>::open(storage_mode.clone())?;

        self.check(storage_mode)?;

        let height = committee.current_height()?;
        let my_height = self.height();

        // the act of creating this ledger service with a "max_gc_rounds" set to 0 should clear
        // all BFT documents
        let ledger_service = Arc::new(CoreLedgerService::new(ledger.clone(), Default::default()));
        Storage::new(ledger_service, Arc::new(BFTMemoryService::new()), 0);

        // TODO: parallel and test out of order removal by moving the guts of these functions out of the "atomic writes"
        for h in ((my_height + 1)..=height).rev() {
            if let Some(hash) = blocks.get_block_hash(h)? {
                blocks.remove(&hash)?;
                committee.remove(h)?;
            };
        }

        // TODO: diff the programs so we don't have to remove the mappings

        // delete old mappings (can make this parallel)
        for (prog, mappings) in finalize.program_id_map().iter_confirmed() {
            for mapping in mappings.iter() {
                finalize.remove_mapping(*prog, *mapping)?;
            }
        }

        // write replacement mappings
        for ((prog, mapping), entries) in self.content.key_values.into_iter() {
            finalize.initialize_mapping(prog, mapping)?;
            finalize.replace_mapping(prog, mapping, entries)?;
        }

        // set the current round to the last round in the new top block
        // using the committee store to determine what the first round of the new top block is
        if let Some(c) = committee.get_committee(my_height)? {
            let mut round = c.starting_round();
            // loop until the the next round is different (it will be None, but this is cleaner)
            while committee.get_height_for_round(round + 1)? == Some(height) {
                round += 1;
            }
            committee.current_round_map().insert(ROUND_KEY, round)?;
        } else {
            bail!("no committee found for height {my_height}. ledger is likely corrupted",);
        }

        Ok(())
    }

    pub fn height(&self) -> u32 {
        self.header.block_height
    }

    pub fn header(&self) -> &CheckpointHeader<N> {
        &self.header
    }
}
