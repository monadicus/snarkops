pub use aleo_std::StorageMode;
pub use snarkos_node::bft::{
    helpers::Storage, ledger_service::CoreLedgerService, storage_service::BFTMemoryService,
};
pub use snarkvm::{
    console::{
        network::MainnetV0,
        program::{self, Itertools},
    },
    ledger::{
        store::{
            helpers::{
                rocksdb::{self, *},
                Map, MapRead,
            },
            *,
        },
        Ledger,
    },
    utilities::{FromBytes, ToBytes},
};
pub type N = MainnetV0;
pub type Db = ConsensusDB<N>;
pub type DbLedger = Ledger<N, Db>;
pub type TransitionID = <N as snarkvm::prelude::Network>::TransitionID;
pub type TransactionID = <N as snarkvm::prelude::Network>::TransactionID;
pub type BlockHash = <N as snarkvm::prelude::Network>::BlockHash;
pub type BlockDB = rocksdb::BlockDB<N>;
pub type FinalizeDB = rocksdb::FinalizeDB<N>;
pub type CommitteeDB = rocksdb::CommitteeDB<N>;
pub type ProgramID = program::ProgramID<N>;
pub type Identifier = program::Identifier<N>;
pub type Plaintext = program::Plaintext<N>;
pub type Value = program::Value<N>;

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
