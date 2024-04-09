pub use aleo_std::StorageMode;
pub use snarkos_node::bft::{
    helpers::Storage, ledger_service::CoreLedgerService, storage_service::BFTMemoryService,
};
use snarkvm::{
    console::{network::MainnetV0, program},
    ledger::{store::helpers::rocksdb, Ledger},
    prelude::Network,
};
pub use snarkvm::{
    ledger::{
        authority::Authority,
        store::{
            self, cow_to_cloned, cow_to_copied,
            helpers::{Map, MapRead},
            BlockStorage, CommitteeStorage, DeploymentStorage, ExecutionStorage, FeeStorage,
            FinalizeStorage, InputStorage, OutputStorage, TransactionStorage, TransactionType,
            TransitionStorage,
        },
    },
    utilities::{FromBytes, ToBytes},
};

pub type N = MainnetV0;
pub type Db = rocksdb::ConsensusDB<N>;
pub type DbLedger = Ledger<N, Db>;

pub type TransitionID = <N as Network>::TransitionID;
pub type TransactionID = <N as Network>::TransactionID;
pub type BlockHash = <N as Network>::BlockHash;

pub type ProgramID = program::ProgramID<N>;
pub type Identifier = program::Identifier<N>;
pub type Plaintext = program::Plaintext<N>;
pub type Value = program::Value<N>;

pub type BlockDB = rocksdb::BlockDB<N>;
pub type CommitteeDB = rocksdb::CommitteeDB<N>;
pub type DeploymentDB = rocksdb::DeploymentDB<N>;
pub type ExecutionDB = rocksdb::ExecutionDB<N>;
pub type FeeDB = rocksdb::FeeDB<N>;
pub type FinalizeDB = rocksdb::FinalizeDB<N>;
pub type InputDB = rocksdb::InputDB<N>;
pub type OutputDB = rocksdb::OutputDB<N>;
pub type TransactionDB = rocksdb::TransactionDB<N>;
pub type TransitionDB = rocksdb::TransitionDB<N>;

pub type TransitionStore = store::TransitionStore<N, TransitionDB>;
pub type FeeStore = store::FeeStore<N, FeeDB>;

pub trait LazyBytes {
    fn bytes(&self) -> [u8; 32];
}
impl LazyBytes for BlockHash {
    fn bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&self.to_bytes_le().unwrap());
        bytes
    }
}
