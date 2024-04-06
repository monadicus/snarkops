use chrono::{DateTime, TimeDelta, Utc};
use std::{
    io::{self, Read, Write},
    path::PathBuf,
};

use crate::CheckpointHeaderError::{self as Error, *};

const CHECKPOINT_VERSION: u8 = 2;

#[derive(Debug, Clone)]
pub struct CheckpointHeader {
    /// Block height
    pub block_height: u32,
    /// Block timestamp
    pub timestamp: i64,
    /// Block's hash
    pub block_hash: [u8; 32],
    /// Genesis block's hash - used to ensure the checkpoint is applicable to this network
    pub genesis_hash: [u8; 32],
    /// Size of the checkpoint
    pub content_len: u64,
}

impl CheckpointHeader {
    pub fn read_file(path: &PathBuf) -> Result<Self, Error> {
        let reader = std::fs::File::options()
            .read(true)
            .open(path)
            .map_err(FileError)?;
        let header = Self::read_bytes(&reader).map_err(ReadError)?;
        Ok(header)
    }

    #[cfg(feature = "write")]
    pub fn read_ledger(path: PathBuf) -> Result<Self, Error> {
        use crate::snarkos::{
            BlockDB, BlockStorage, CommitteeDB, CommitteeStorage, LazyBytes, StorageMode,
        };
        use Error::*;

        let commitee = CommitteeDB::open(StorageMode::Custom(path.clone())).map_err(OpenLedger)?;
        let blocks = BlockDB::open(StorageMode::Custom(path)).map_err(OpenLedger)?;

        let height = commitee.current_height().map_err(ReadLedger)?;
        let Some(block_hash) = blocks.get_block_hash(height).map_err(ReadLedger)? else {
            return Err(BlockNotFound(height));
        };
        let Some(genesis_hash) = blocks.get_block_hash(0).map_err(ReadLedger)? else {
            return Err(HashlessGenesis);
        };
        let Some(block_header) = blocks.get_block_header(&block_hash).map_err(ReadLedger)? else {
            return Err(BlockMissingHeader(height, block_hash.to_string()));
        };

        Ok(Self {
            block_height: height,
            timestamp: block_header.timestamp(),
            block_hash: block_hash.bytes(),
            genesis_hash: genesis_hash.bytes(),
            content_len: 0,
        })
    }

    pub fn time(&self) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + TimeDelta::new(self.timestamp, 0).unwrap()
    }

    pub fn write_bytes<W: Write>(&self, mut w: W) -> io::Result<()> {
        w.write_all(&[CHECKPOINT_VERSION])?;
        w.write_all(&self.block_height.to_le_bytes())?;
        w.write_all(&self.timestamp.to_le_bytes())?;
        w.write_all(&self.block_hash)?;
        w.write_all(&self.genesis_hash)?;
        w.write_all(&self.content_len.to_le_bytes())?;
        Ok(())
    }

    pub fn read_bytes<R: Read>(mut r: R) -> io::Result<Self> {
        let mut buf = [0u8; 1 + 4 + 8 + 32 + 32 + 8];
        r.read_exact(&mut buf)?;
        let mut buf = buf.into_iter();

        let version = buf.next().unwrap();
        if version != CHECKPOINT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                format!("invalid checkpoint version: {version}, expected {CHECKPOINT_VERSION}"),
            ));
        }

        fn take<const SIZE: usize>(buf: &mut impl Iterator<Item = u8>, n: usize) -> [u8; SIZE] {
            let mut arr = [0u8; SIZE];
            buf.take(n).enumerate().for_each(|(i, b)| arr[i] = b);
            arr
        }

        let block_height = u32::from_le_bytes(take(&mut buf, 4));
        let timestamp = i64::from_le_bytes(take(&mut buf, 8));
        let block_hash = take(&mut buf, 32);
        let genesis_hash = take(&mut buf, 32);
        let content_len = u64::from_le_bytes(take(&mut buf, 8));

        Ok(Self {
            block_height,
            timestamp,
            block_hash,
            genesis_hash,
            content_len,
        })
    }
}
