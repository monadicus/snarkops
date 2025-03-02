pub use aleo_std::StorageMode;
pub use snarkos_node::bft::{
    helpers::Storage, ledger_service::CoreLedgerService, storage_service::BFTMemoryService,
};
pub use snarkvm::prelude::Network;
use snarkvm::{
    console::program,
    ledger::{Ledger, store::helpers::rocksdb},
};
pub use snarkvm::{
    ledger::{
        authority::Authority,
        store::{
            self, BlockStorage, CommitteeStorage, DeploymentStorage, ExecutionStorage, FeeStorage,
            FinalizeStorage, InputStorage, OutputStorage, TransactionStorage, TransactionType,
            TransitionStorage, cow_to_cloned, cow_to_copied,
            helpers::{Map, MapRead},
        },
    },
    utilities::{FromBytes, ToBytes},
};
pub type Db<N> = rocksdb::ConsensusDB<N>;
pub type DbLedger<N> = Ledger<N, Db<N>>;

pub type TransitionID<N> = <N as Network>::TransitionID;
pub type TransactionID<N> = <N as Network>::TransactionID;
pub type BlockHash<N> = <N as Network>::BlockHash;

pub type ProgramID<N> = program::ProgramID<N>;
pub type Identifier<N> = program::Identifier<N>;
pub type Plaintext<N> = program::Plaintext<N>;
pub type Value<N> = program::Value<N>;

pub type BlockDB<N> = rocksdb::BlockDB<N>;
pub type CommitteeDB<N> = rocksdb::CommitteeDB<N>;
pub type DeploymentDB<N> = rocksdb::DeploymentDB<N>;
pub type ExecutionDB<N> = rocksdb::ExecutionDB<N>;
pub type FeeDB<N> = rocksdb::FeeDB<N>;
pub type FinalizeDB<N> = rocksdb::FinalizeDB<N>;
pub type InputDB<N> = rocksdb::InputDB<N>;
pub type OutputDB<N> = rocksdb::OutputDB<N>;
pub type TransactionDB<N> = rocksdb::TransactionDB<N>;
pub type TransitionDB<N> = rocksdb::TransitionDB<N>;

pub type TransitionStore<N> = store::TransitionStore<N, TransitionDB<N>>;
pub type FeeStore<N> = store::FeeStore<N, FeeDB<N>>;

pub fn block_bytes<N: Network>(block: &BlockHash<N>) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&block.to_bytes_le().unwrap());
    bytes
}
