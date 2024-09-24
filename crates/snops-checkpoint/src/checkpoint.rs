use std::{io, path::PathBuf, sync::Arc};

use rayon::iter::{IntoParallelIterator, ParallelIterator};

use crate::{
    aleo::*,
    errors::{CheckpointCheckError, CheckpointReadError, CheckpointRewindError},
    ledger, CheckpointContent, CheckpointHeader, ROUND_KEY,
};

pub struct Checkpoint<N: Network> {
    pub header: CheckpointHeader,
    pub content: CheckpointContent<N>,
}

impl<N: Network> ToBytes for Checkpoint<N> {
    fn write_le<W: std::io::Write>(&self, mut writer: W) -> std::io::Result<()>
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
        .write_bytes(&mut writer)?;

        writer.write_all(&content_bytes)?;
        Ok(())
    }
}

impl<N: Network> FromBytes for Checkpoint<N> {
    fn read_le<R: std::io::Read>(mut reader: R) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        let header = CheckpointHeader::read_bytes(&mut reader)?;
        let content = CheckpointContent::read_le(&mut reader)?;

        Ok(Self { header, content })
    }
}

impl<N: Network> Checkpoint<N> {
    pub fn new_from_header(
        path: PathBuf,
        header: CheckpointHeader,
    ) -> Result<Self, CheckpointReadError> {
        let content = CheckpointContent::read_ledger(path)?;
        Ok(Self { header, content })
    }

    pub fn new(path: PathBuf) -> Result<Self, CheckpointReadError> {
        let header = CheckpointHeader::read_ledger::<N>(path.clone())?;
        let content = CheckpointContent::read_ledger(path)?;

        Ok(Self { header, content })
    }

    pub fn check(&self, storage_mode: StorageMode) -> Result<(), CheckpointCheckError> {
        use CheckpointCheckError::*;

        let blocks = BlockDB::<N>::open(storage_mode.clone()).map_err(StorageOpenError)?;
        let committee = CommitteeDB::<N>::open(storage_mode.clone()).map_err(StorageOpenError)?;
        let height = committee.current_height().map_err(ReadLedger)?;

        if height <= self.height() {
            return Err(HeightMismatch(self.height(), height));
        }

        let Some(hash): Option<BlockHash<N>> =
            blocks.get_block_hash(self.height()).map_err(ReadLedger)?
        else {
            return Err(BlockNotFound(self.height()));
        };
        if block_bytes::<N>(&hash) != self.header.block_hash {
            return Err(HashMismatch(
                self.height(),
                hash.to_string(),
                BlockHash::<N>::from_bytes_le(&self.header.block_hash)
                    .map(|h| h.to_string())
                    .unwrap_or_else(|_| "invalid hash".to_string()),
            ));
        }

        Ok(())
    }

    pub fn rewind(
        self,
        ledger: &DbLedger<N>,
        storage_mode: StorageMode,
    ) -> Result<(), CheckpointRewindError> {
        use CheckpointRewindError::*;

        let stores = ledger::Stores::open(storage_mode.clone()).map_err(OpenLedger)?;

        self.check(storage_mode)?;

        let height = stores.committee.current_height().map_err(ReadLedger)?;
        let my_height = self.height();

        // the act of creating this ledger service with a "max_gc_rounds" set to 0
        // should clear all BFT documents
        let ledger_service = Arc::new(CoreLedgerService::new(ledger.clone(), Default::default()));
        Storage::new(ledger_service, Arc::new(BFTMemoryService::new()), 0);

        // TODO: parallel and test out of order removal by moving the guts of these
        // functions out of the "atomic writes"
        ((my_height + 1)..=height)
            .into_par_iter()
            .try_for_each(|h| stores.remove(h))
            .map_err(RemoveDocument)?;

        // TODO: diff the programs so we don't have to remove the mappings

        // delete old mappings (can make this parallel)
        for (prog, mappings) in stores.finalize.program_id_map().iter_confirmed() {
            for mapping in mappings.iter() {
                stores
                    .finalize
                    .remove_mapping(*prog, *mapping)
                    .map_err(RemoveDocument)?;
            }
        }

        // write replacement mappings
        for ((prog, mapping), entries) in self.content.key_values.into_iter() {
            stores
                .finalize
                .initialize_mapping(prog, mapping)
                .map_err(RemoveDocument)?;
            stores
                .finalize
                .replace_mapping(prog, mapping, entries)
                .map_err(RemoveDocument)?;
        }

        // set the current round to the last round in the new top block
        // using the committee store to determine what the first round of the new top
        // block is
        if let Some(c) = stores
            .committee
            .get_committee(my_height)
            .map_err(RemoveDocument)?
        {
            let mut round = c.starting_round();
            // loop until the the next round is different (it will be None, but this is
            // cleaner)
            while stores
                .committee
                .get_height_for_round(round + 1)
                .map_err(RemoveDocument)?
                == Some(height)
            {
                round += 1;
            }
            stores
                .committee
                .current_round_map()
                .insert(ROUND_KEY, round)
                .map_err(RemoveDocument)?;
        } else {
            return Err(MissingCommittee(my_height));
        }

        Ok(())
    }

    pub fn height(&self) -> u32 {
        self.header.block_height
    }

    pub fn header(&self) -> &CheckpointHeader {
        &self.header
    }
}
