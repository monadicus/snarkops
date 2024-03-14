use snarkvm::{
    console::network::MainnetV0,
    ledger::{
        store::helpers::{memory::ConsensusMemory, rocksdb::ConsensusDB},
        Ledger,
    },
};

pub type Network = MainnetV0;
pub type Db = ConsensusDB<Network>;
pub type MemoryLedger = Ledger<Network, ConsensusMemory<Network>>;
pub type DbLedger = Ledger<Network, Db>;
