use std::{
    collections::HashMap,
    sync::{atomic::AtomicU64, Arc},
};

use bimap::BiHashMap;
use chrono::{DateTime, TimeDelta, Utc};
use lazy_static::lazy_static;
use snops_common::state::{LatestBlockInfo, NodeKey};

lazy_static! {
    static ref TX_COUNTER: AtomicU64 = AtomicU64::new(0);
}

pub type ABlockHash = Arc<str>;
pub type ATransactionId = Arc<str>;

/// The maximum number of block distance from the tip before we don't consider
/// fetching it
pub const MAX_BLOCK_RANGE: u32 = 10;

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
    /// A map of external peer node keys to their latest block info.
    ///
    /// This is not a map of Arc<LatestBlockInfo> because we want to update the
    /// timestamp when we update the info.
    pub external_peer_infos: HashMap<NodeKey, LatestBlockInfo>,
    /// A map of the peer's "track record" for responsiveness.
    pub external_peer_record: HashMap<NodeKey, ResponsiveRecord>,
    /// The most recent block info
    pub latest: Option<LatestBlockInfo>,
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

    pub fn update_peer_req(&mut self, key: &NodeKey, success: bool) {
        let record = self
            .external_peer_record
            .entry(key.clone())
            .or_insert(ResponsiveRecord {
                failed_attempts: 0,
                total_successes: 0,
                last_attempt: Utc::now(),
                last_success: Utc::now(),
            });

        if success {
            record.reward();
        } else {
            record.punish();
        }
    }

    pub fn is_peer_penalized(&self, key: &NodeKey) -> bool {
        self.external_peer_record
            .get(key)
            .map_or(false, ResponsiveRecord::has_penalty)
    }

    /// Update a peer's node info if the provided block hash exists in the cache
    pub fn update_peer_info_for_hash(&mut self, key: &NodeKey, hash: &str) {
        // ensure info exists
        let Some(info) = self.blocks.get(hash) else {
            return;
        };

        // ensure the hash is different
        if self
            .external_peer_infos
            .get(key)
            .is_some_and(|i| i.block_hash == hash)
        {
            return;
        }

        // update the info has with a new timestamp
        let mut info = info.clone();
        info.update_time = Utc::now();
        self.update_peer_info(key.clone(), info);
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

    /// Add a block to the cache
    pub fn add_block(&mut self, block_info: LatestBlockInfo, txs: Vec<ATransactionId>) {
        let hash = Arc::from(block_info.block_hash.as_ref());
        self.height_and_hash
            .insert(block_info.height, Arc::clone(&hash));
        for tx in &txs {
            self.transaction_to_block_hash
                .insert(Arc::clone(tx), Arc::clone(&hash));
        }
        self.block_to_transaction.insert(
            Arc::clone(&hash),
            TransactionCache {
                create_time: block_info.update_time,
                entries: txs,
            },
        );
        self.blocks.insert(Arc::clone(&hash), block_info);
    }

    /// Remove a block from the cache
    pub fn remove_block(&mut self, block_hash: &ABlockHash) {
        self.height_and_hash.retain(|_, v| v != block_hash);
        self.block_to_transaction.remove(block_hash);
        self.transaction_to_block_hash
            .retain(|_, v| v != block_hash);
        self.blocks.remove(block_hash);
    }
}

/// A record of a peer's responsiveness for avoiding using peers that are
/// unresponsive
pub struct ResponsiveRecord {
    /// Number of **consecutive** failed attempts to reach the peer
    pub failed_attempts: u32,
    /// Number of successful attempts to reach the peer
    pub total_successes: u32,
    /// The last time an attempt was made to reach the peer
    pub last_attempt: DateTime<Utc>,
    /// The last time the peer was successfully reached
    pub last_success: DateTime<Utc>,
}

impl ResponsiveRecord {
    /// The maximum penalty time for a peer
    pub const MAX_PENALTY: u32 = 60 * 60;

    /// Time to wait before attempting to reach the peer again
    pub fn penalty(&self) -> Option<TimeDelta> {
        if self.failed_attempts == 0 {
            return None;
        }

        // The penalty is based on the time since the last successful attempt.
        // The longer the time since the last success, the longer the penalty
        Some(self.last_success - self.last_attempt)
    }

    /// Whether the peer is currently penalized
    pub fn has_penalty(&self) -> bool {
        let Some(penalty) = self.penalty() else {
            return false;
        };
        self.last_attempt + penalty > Utc::now()
    }

    /// Punish the peer for failing to respond
    pub fn punish(&mut self) {
        self.failed_attempts += 1;
        self.last_attempt = Utc::now();
    }

    /// Reward the peer for responding
    pub fn reward(&mut self) {
        self.total_successes += 1;
        self.failed_attempts = 0;
        self.last_success = Utc::now();
        self.last_attempt = Utc::now();
    }
}
