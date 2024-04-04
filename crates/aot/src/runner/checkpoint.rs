use std::{io, sync::Arc};

use aleo_std::StorageMode;
use anyhow::{bail, ensure, Result};
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
const CHECKPOINT_VERSION: u8 = 1;

pub struct CheckpointHeader<N: NetworkTrait> {
    /// Block height
    pub block_height: u32,
    /// Block timestamp
    pub timestamp: i64,
    /// Block's hash
    pub block_hash: N::BlockHash,
    /// Genesis block's hash - used to ensure the checkpoint is applicable to this network
    pub genesis_hash: N::BlockHash,
    /// Size of the checkpoint
    pub content_len: u64,
}

impl<N: NetworkTrait> ToBytes for CheckpointHeader<N> {
    fn write_le<W: snarkvm::prelude::Write>(&self, mut writer: W) -> snarkvm::prelude::IoResult<()>
    where
        Self: Sized,
    {
        CHECKPOINT_VERSION.write_le(&mut writer)?;
        self.block_height.write_le(&mut writer)?;
        self.timestamp.write_le(&mut writer)?;
        self.block_hash.write_le(&mut writer)?;
        self.genesis_hash.write_le(&mut writer)?;
        self.content_len.write_le(&mut writer)?;
        Ok(())
    }
}

impl<N: NetworkTrait> FromBytes for CheckpointHeader<N> {
    fn read_le<R: snarkvm::prelude::Read>(mut reader: R) -> snarkvm::prelude::IoResult<Self>
    where
        Self: Sized,
    {
        let version = u8::read_le(&mut reader)?;
        if version != CHECKPOINT_VERSION {
            return snarkvm::prelude::IoResult::Err(io::Error::new(
                io::ErrorKind::Interrupted,
                format!("invalid checkpoint version: {version}, expected {CHECKPOINT_VERSION}"),
            ));
        }

        let block_height = u32::read_le(&mut reader)?;
        let timestamp = i64::read_le(&mut reader)?;
        let block_hash = N::BlockHash::read_le(&mut reader)?;
        let genesis_hash = N::BlockHash::read_le(&mut reader)?;
        let content_len = u64::read_le(&mut reader)?;

        Ok(Self {
            block_height,
            timestamp,
            block_hash,
            genesis_hash,
            content_len,
        })
    }
}

/// Storage of key-value pairs for each program ID and identifier
/// Note, the structure is this way as ToBytes derives 2 sized tuples, but not 3 sized tuples
pub struct CheckpointContent<N: NetworkTrait> {
    #[allow(clippy::type_complexity)]
    key_values: Vec<((ProgramID<N>, Identifier<N>), Vec<(Plaintext<N>, Value<N>)>)>,
}

impl<N: NetworkTrait> ToBytes for CheckpointContent<N> {
    fn write_le<W: snarkvm::prelude::Write>(&self, mut writer: W) -> snarkvm::prelude::IoResult<()>
    where
        Self: Sized,
    {
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

impl<N: NetworkTrait> FromBytes for CheckpointContent<N> {
    fn read_le<R: snarkvm::prelude::Read>(mut reader: R) -> snarkvm::prelude::IoResult<Self>
    where
        Self: Sized,
    {
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

        Ok(Self { key_values })
    }
}

pub struct Checkpoint<N: NetworkTrait> {
    header: CheckpointHeader<N>,
    content: CheckpointContent<N>,
}

impl<N: NetworkTrait> ToBytes for Checkpoint<N> {
    fn write_le<W: snarkvm::prelude::Write>(&self, mut writer: W) -> snarkvm::prelude::IoResult<()>
    where
        Self: Sized,
    {
        let content_bytes = self.content.to_bytes_le().map_err(|e| {
            io::Error::new(
                io::ErrorKind::Interrupted,
                format!("error serializing content: {e}"),
            )
        })?;

        CheckpointHeader {
            content_len: content_bytes.len() as u64,
            ..self.header
        }
        .write_le(&mut writer)?;

        writer.write_all(&content_bytes)?;
        Ok(())
    }
}

impl<N: NetworkTrait> FromBytes for Checkpoint<N> {
    fn read_le<R: snarkvm::prelude::Read>(mut reader: R) -> snarkvm::prelude::IoResult<Self>
    where
        Self: Sized,
    {
        let header = CheckpointHeader::read_le(&mut reader)?;
        let content = CheckpointContent::read_le(&mut reader)?;

        Ok(Self { header, content })
    }
}

impl<N: NetworkTrait> Checkpoint<N> {
    pub fn new(storage_mode: StorageMode) -> Result<Self> {
        let commitee = CommitteeDB::<N>::open(storage_mode.clone())?;
        let finalize = FinalizeDB::<N>::open(storage_mode.clone())?;
        let blocks = BlockDB::<N>::open(storage_mode.clone())?;

        let height = commitee.current_height()?;
        let Some(block_hash) = blocks.get_block_hash(height)? else {
            bail!("no block found at height {height}");
        };
        let Some(genesis_hash) = blocks.get_block_hash(0)? else {
            bail!("genesis block missing a hash... somehow");
        };
        let Some(block_header) = blocks.get_block_header(&block_hash)? else {
            bail!("no block header found for block hash {block_hash} at height {height}");
        };

        // let timestamp = blocks.

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

        let header = CheckpointHeader {
            block_height: height,
            timestamp: block_header.timestamp(),
            block_hash,
            genesis_hash,
            content_len: 0,
        };

        Ok(Self {
            header,
            content: CheckpointContent { key_values },
        })
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
