pub mod cli;
pub mod genesis;
pub mod ledger;

#[cfg(feature = "node")]
pub mod runner;

use snarkvm::{
    console::network::MainnetV0,
    ledger::{
        store::helpers::{memory::ConsensusMemory, rocksdb::ConsensusDB},
        Ledger,
    },
};

pub type Network = MainnetV0;
pub type TransactionID = <Network as snarkvm::prelude::Network>::TransactionID;
pub type Db = ConsensusDB<Network>;
pub type MemoryLedger = Ledger<Network, ConsensusMemory<Network>>;
pub type DbLedger = Ledger<Network, Db>;
pub type PrivateKey = snarkvm::console::account::PrivateKey<Network>;
pub type Account = snarkos_account::Account<Network>;
pub type Address = snarkvm::console::types::Address<Network>;
pub type VM = snarkvm::synthesizer::VM<Network, Db>;
