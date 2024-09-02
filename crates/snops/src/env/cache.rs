use std::{
    collections::HashMap,
    sync::{atomic::AtomicU64, Arc},
};

use bimap::BiHashMap;
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use snops_common::state::{LatestBlockInfo, NodeKey};

lazy_static! {
    static ref TX_COUNTER: AtomicU64 = AtomicU64::new(0);
}

pub type ABlockHash = Arc<str>;
pub type ATransactionId = Arc<str>;

/// Exists per environment to track transactions for the most recent blocks
/// TODO: task to prune old/unused data
#[derive(Default)]
pub struct NetworkCache {
    /// BiMap of block height to block hash
    pub height_and_hash: BiHashMap<u32, ABlockHash>,
    /// BiMap of block hash to transaction ids
    pub block_to_transaction: HashMap<ABlockHash, TransactionCache>,
    /// Lookup for block hashes given a transaction id
    pub transaction_to_block_hash: HashMap<ATransactionId, ABlockHash>,
    /// A map of block height to block info
    pub blocks: HashMap<ABlockHash, LatestBlockInfo>,
    // A map of external peer node keys to their latest block info
    pub external_peer_infos: HashMap<NodeKey, LatestBlockInfo>,
    /// The most recent block info
    pub latest: Option<LatestBlockInfo>,
    /// The height of the highest block with transaction data
    pub max_block_height: Option<u32>,
}

/// A list of transactions paired with the time they were added to the cache
#[derive(Default)]
pub struct TransactionCache {
    /// Time this cache was created
    pub create_time: DateTime<Utc>,
    /// List of transaction ids in this block
    pub entries: Vec<ATransactionId>,
}

impl NetworkCache {
    pub fn update_latest_info(&mut self, info: &LatestBlockInfo) {
        match &self.latest {
            Some(prev) if prev.block_timestamp < info.block_timestamp => {
                self.latest.replace(info.clone());
            }
            None => {
                self.latest = Some(info.clone());
            }
            _ => {}
        }
    }

    pub fn update_peer_info(&mut self, key: NodeKey, info: LatestBlockInfo) {
        self.external_peer_infos.insert(key, info);
    }

    pub fn is_block_stale(&self, _block_hash: &ABlockHash) -> bool {
        // TODO: check if the block_to_transaction timestamp's age is greater than N
        // TODO: if there is block_to_transaction, check if block's info's age is
        // greater than N
        false
    }

    pub fn remove_block(&mut self, block_hash: &ABlockHash) {
        self.height_and_hash.retain(|_, v| v != block_hash);
        self.block_to_transaction.remove(block_hash);
        self.transaction_to_block_hash
            .retain(|_, v| v != block_hash);
        self.blocks.remove(block_hash);
    }
}
