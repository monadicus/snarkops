use std::sync::Arc;

use aleo_std::StorageMode;
use anyhow::{bail, Result};
use snarkos_node::bft::{
    helpers::Storage, ledger_service::CoreLedgerService, storage_service::BFTMemoryService,
};
use snarkvm::{
    console::program::{Identifier, Network as NetworkTrait, Plaintext, ProgramID, Value},
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
    utilities::{FromBytes, ToBytes},
};

/// Committee store round key (this will probably never change)
const ROUND_KEY: u8 = 0;

pub struct Checkpoint<N: NetworkTrait> {
    pub height: u32,
    /// Storage of key-value pairs for each program ID and identifier
    /// Note, the structure is this way as ToBytes derives 2 sized tuples, but not 3 sized tuples
    #[allow(clippy::type_complexity)]
    key_values: Vec<((ProgramID<N>, Identifier<N>), Vec<(Plaintext<N>, Value<N>)>)>,
}

impl<N: NetworkTrait> ToBytes for Checkpoint<N> {
    fn write_le<W: snarkvm::prelude::Write>(&self, mut writer: W) -> snarkvm::prelude::IoResult<()>
    where
        Self: Sized,
    {
        self.height.write_le(&mut writer)?;

        // the default vec writer does not include the length
        (self.key_values.len() as u64).write_le(&mut writer)?;
        for (key, entries) in &self.key_values {
            key.write_le(&mut writer)?;
            (entries.len() as u64).write_le(&mut writer)?;
            entries.write_le(&mut writer)?;
        }
        Ok(())
    }
}

impl<N: NetworkTrait> FromBytes for Checkpoint<N> {
    fn read_le<R: snarkvm::prelude::Read>(mut reader: R) -> snarkvm::prelude::IoResult<Self>
    where
        Self: Sized,
    {
        let height = u32::read_le(&mut reader)?;

        let len = u64::read_le(&mut reader)?;
        let mut key_values = Vec::with_capacity(len as usize);

        for _ in 0..len {
            let key = <(ProgramID<N>, Identifier<N>)>::read_le(&mut reader)?;
            let len = u64::read_le(&mut reader)?;
            let mut entries = Vec::with_capacity(len as usize);
            for _ in 0..len {
                entries.push(<(Plaintext<N>, Value<N>)>::read_le(&mut reader)?);
            }
            key_values.push((key, entries));
        }

        Ok(Self { height, key_values })
    }
}

impl<N: NetworkTrait> Checkpoint<N> {
    pub fn new(height: u32, storage_mode: StorageMode) -> Result<Self> {
        let finalize = FinalizeDB::<N>::open(storage_mode)?;

        let key_values = finalize
            .program_id_map()
            .iter_confirmed()
            .map(|(prog, mappings)| {
                mappings
                    .iter()
                    .map(|mapping| {
                        finalize
                            .get_mapping_confirmed(*prog, *mapping)
                            .map(|entries| ((*prog, *mapping), entries))
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .flat_map(|res| match res {
                // yeah... you have to collect again just for this to work
                Ok(v) => v.into_iter().map(Ok).collect::<Vec<_>>(),
                Err(e) => {
                    tracing::error!("error reading key-values: {e}");
                    vec![Err(e)]
                }
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self { height, key_values })
    }

    #[allow(unused)]
    pub fn rewind(
        self,
        ledger: &Ledger<N, ConsensusDB<N>>,
        storage_mode: StorageMode,
    ) -> Result<()> {
        use rayon::iter::ParallelIterator;

        let finalize = FinalizeDB::<N>::open(storage_mode.clone())?;
        let blocks = BlockDB::<N>::open(storage_mode.clone())?;
        let committee = CommitteeDB::<N>::open(storage_mode.clone())?;

        let height = committee.current_height()?;

        assert!(self.height < height);

        // the act of creating this ledger service with a "max_gc_rounds" set to 0 should clear
        // all BFT documents
        let ledger_service = Arc::new(CoreLedgerService::new(ledger.clone(), Default::default()));
        Storage::new(ledger_service, Arc::new(BFTMemoryService::new()), 0);

        // TODO: parallel and test out of order removal by moving the guts of these functions out of the "atomic writes"
        for h in ((self.height + 1)..=height).rev() {
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
        for ((prog, mapping), entries) in self.key_values.into_iter() {
            finalize.initialize_mapping(prog, mapping)?;
            finalize.replace_mapping(prog, mapping, entries)?;
        }

        // set the current round to the last round in the new top block
        // using the committee store to determine what the first round of the new top block is
        if let Some(c) = committee.get_committee(self.height)? {
            let mut round = c.starting_round();
            // loop until the the next round is different (it will be None, but this is cleaner)
            while committee.get_height_for_round(round + 1)? == Some(height) {
                round += 1;
            }
            committee.current_round_map().insert(ROUND_KEY, round)?;
        } else {
            bail!(
                "no committee found for height {}. ledger is likely corrupted",
                self.height
            );
        }

        Ok(())
    }
}
