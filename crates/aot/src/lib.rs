pub mod cli;
pub mod genesis;
pub mod ledger;
pub mod node;

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
pub type PrivateKey = snarkvm::console::account::PrivateKey<Network>;
pub type Account = snarkvm::console::account::Account<Network>;
pub type Address = snarkvm::console::types::Address<Network>;
pub type VM = snarkvm::synthesizer::VM<Network, Db>;
