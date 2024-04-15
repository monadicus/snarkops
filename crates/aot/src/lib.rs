pub mod accounts;
pub mod authorized;
pub mod cli;
pub mod credits;
pub mod genesis;
pub mod ledger;

#[cfg(feature = "node")]
pub mod runner;

use rand::SeedableRng;
use rand_chacha::ChaChaRng;
use snarkvm::{
    console::network::MainnetV0,
    ledger::{
        store::helpers::{memory::ConsensusMemory, rocksdb::ConsensusDB},
        Ledger,
    },
};

// The current network.
pub type Aleo = snarkvm::circuit::AleoV0;
pub type Network = MainnetV0;

// The db.
pub type Db = ConsensusDB<Network>;

// Ledger types.
pub type DbLedger = Ledger<Network, Db>;
pub type MemoryLedger = Ledger<Network, ConsensusMemory<Network>>;

// The vm types.
pub type MemVM = snarkvm::synthesizer::VM<Network, ConsensusMemory<Network>>;
pub type VM = snarkvm::synthesizer::VM<Network, Db>;

// Tx types.
pub type TransactionID = <Network as snarkvm::prelude::Network>::TransactionID;
pub type Transaction = snarkvm::ledger::Transaction<Network>;

// Account types.
pub type Account = snarkos_account::Account<Network>;

// User types.
pub type Address = snarkvm::console::types::Address<Network>;
pub type PrivateKey = snarkvm::console::account::PrivateKey<Network>;
pub type ViewKey = snarkvm::console::account::ViewKey<Network>;

// Value types.
// Text types.
pub type Ciphertext = snarkvm::console::program::Ciphertext<Network>;
pub type Plaintext = snarkvm::console::program::Plaintext<Network>;

// Record types.
pub type CTRecord = snarkvm::console::program::Record<Network, Ciphertext>;
pub type PTRecord = snarkvm::console::program::Record<Network, Plaintext>;

// Other types.
pub type Value = snarkvm::console::program::Value<Network>;
pub type Literal = snarkvm::console::program::Literal<Network>;

// Program types.
pub type Authorization = snarkvm::synthesizer::Authorization<Network>;
pub type Block = snarkvm::ledger::Block<Network>;
pub type Committee = snarkvm::ledger::committee::Committee<Network>;

pub fn gen_private_key() -> anyhow::Result<PrivateKey> {
    let mut rng = ChaChaRng::from_entropy();
    PrivateKey::new(&mut rng)
}
