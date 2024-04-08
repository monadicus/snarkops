use std::path::PathBuf;

use anyhow::Result;

use crate::{
    aleo::{
        FinalizeDB, FinalizeStorage, FromBytes, Identifier, MapRead, Plaintext, ProgramID,
        StorageMode, ToBytes, Value,
    },
    errors::CheckpointContentError as Error,
};

/// Committee store round key (this will probably never change)
pub const ROUND_KEY: u8 = 0;

/// Storage of key-value pairs for each program ID and identifier
/// Note, the structure is this way as ToBytes derives 2 sized tuples, but not 3
/// sized tuples
pub struct CheckpointContent {
    #[allow(clippy::type_complexity)]
    pub key_values: Vec<((ProgramID, Identifier), Vec<(Plaintext, Value)>)>,
}

impl CheckpointContent {
    pub fn read_ledger(path: PathBuf) -> Result<Self, Error> {
        use Error::*;

        let finalize = FinalizeDB::open(StorageMode::Custom(path)).map_err(OpenLedger)?;
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
            .collect::<Result<Vec<_>>>()
            .map_err(ReadLedger)?;

        Ok(Self { key_values })
    }
}

impl ToBytes for CheckpointContent {
    fn write_le<W: std::io::Write>(&self, mut writer: W) -> std::io::Result<()>
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

impl FromBytes for CheckpointContent {
    fn read_le<R: std::io::Read>(mut reader: R) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        let len = u64::read_le(&mut reader)?;
        let mut key_values = Vec::with_capacity(len as usize);

        for _ in 0..len {
            let key = <(ProgramID, Identifier)>::read_le(&mut reader)?;
            let len = u64::read_le(&mut reader)?;
            let mut entries = Vec::with_capacity(len as usize);
            for _ in 0..len {
                entries.push(<(Plaintext, Value)>::read_le(&mut reader)?);
            }
            key_values.push((key, entries));
        }

        Ok(Self { key_values })
    }
}
