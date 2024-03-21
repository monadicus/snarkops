pub mod cli;
pub mod credits;
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
pub type MemVM = snarkvm::synthesizer::VM<Network, ConsensusMemory<Network>>;
pub type Plaintext = snarkvm::console::program::Plaintext<Network>;
pub type Ciphertext = snarkvm::console::program::Ciphertext<Network>;
pub type PTRecord = snarkvm::console::program::Record<Network, Plaintext>;
pub type CTRecord = snarkvm::console::program::Record<Network, Ciphertext>;
pub type ViewKey = snarkvm::console::account::ViewKey<Network>;
pub type Value = snarkvm::console::program::Value<Network>;
pub type Literal = snarkvm::console::program::Literal<Network>;
pub type Authorization = snarkvm::synthesizer::Authorization<Network>;
pub type Aleo = snarkvm::circuit::AleoV0;
pub type Transaction = snarkvm::ledger::Transaction<Network>;
