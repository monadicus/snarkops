use aleo_std::StorageMode;
use anyhow::Result;
use snarkvm::{
    console::program::{Identifier, Network as NetworkTrait, Plaintext, ProgramID, Value},
    ledger::store::{
        helpers::{rocksdb::FinalizeDB, MapRead},
        FinalizeStorage,
    },
    utilities::{FromBytes, ToBytes},
};
use std::{io, path::PathBuf};

use super::CheckpointHeader;

/// Committee store round key (this will probably never change)
pub const ROUND_KEY: u8 = 0;

/// Storage of key-value pairs for each program ID and identifier
/// Note, the structure is this way as ToBytes derives 2 sized tuples, but not 3 sized tuples
pub struct CheckpointContent<N: NetworkTrait> {
    #[allow(clippy::type_complexity)]
    pub key_values: Vec<((ProgramID<N>, Identifier<N>), Vec<(Plaintext<N>, Value<N>)>)>,
}

impl<N: NetworkTrait> CheckpointContent<N> {
    pub fn read_ledger(path: PathBuf) -> Result<Self> {
        let finalize = FinalizeDB::<N>::open(StorageMode::Custom(path))?;
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

        Ok(Self { key_values })
    }
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
    pub header: CheckpointHeader<N>,
    pub content: CheckpointContent<N>,
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
