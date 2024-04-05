use anyhow::Result;
use snarkvm::{
    console::program::Network as NetworkTrait,
    utilities::{FromBytes, ToBytes},
};
use std::{io, path::Path};

use crate::Network;

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

impl<N: NetworkTrait> CheckpointHeader<N> {
    pub fn read_file(path: &Path) -> Result<Self> {
        let reader = std::fs::File::options().read(true).open(path)?;
        let header = Self::read_le(&reader)?;
        Ok(header)
    }
}
