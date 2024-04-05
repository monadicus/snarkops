use aleo_std::StorageMode;
use anyhow::{bail, Result};
use chrono::{DateTime, TimeDelta, Utc};
use snarkvm::{
    console::program::Network as NetworkTrait,
    ledger::store::{
        helpers::rocksdb::{BlockDB, CommitteeDB},
        BlockStorage, CommitteeStorage,
    },
    utilities::{FromBytes, ToBytes},
};
use std::{io, path::PathBuf};

const CHECKPOINT_VERSION: u8 = 1;

#[derive(Debug, Clone)]
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
    pub fn read_file(path: &PathBuf) -> Result<Self> {
        let reader = std::fs::File::options().read(true).open(path)?;
        let header = Self::read_le(&reader)?;
        Ok(header)
    }

    pub fn read_ledger(path: PathBuf) -> Result<Self> {
        let commitee = CommitteeDB::<N>::open(StorageMode::Custom(path.clone()))?;
        let blocks = BlockDB::<N>::open(StorageMode::Custom(path))?;

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

        Ok(Self {
            block_height: height,
            timestamp: block_header.timestamp(),
            block_hash,
            genesis_hash,
            content_len: 0,
        })
    }

    pub fn time(&self) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + TimeDelta::new(self.timestamp, 0).unwrap()
    }
}
