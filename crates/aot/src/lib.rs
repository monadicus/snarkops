pub mod accounts;
pub mod auth;
pub mod cli;
pub mod genesis;
pub mod ledger;

#[cfg(feature = "node")]
pub mod runner;

use std::sync::OnceLock;

use rand::SeedableRng;
use rand_chacha::ChaChaRng;
use snarkvm::{
    circuit::{Aleo, AleoCanaryV0, AleoTestnetV0, AleoV0},
    console::{
        network::{CanaryV0, MainnetV0, TestnetV0},
        program::Network as NetworkTrait,
    },
    ledger::{
        store::helpers::{memory::ConsensusMemory, rocksdb::ConsensusDB},
        Ledger,
    },
    synthesizer::Process,
};

pub enum NetworkId {
    Mainnet,
    Testnet,
    Canary,
}

impl std::str::FromStr for NetworkId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(Self::Mainnet),
            "testnet" => Ok(Self::Testnet),
            "canary" => Ok(Self::Canary),
            _ => Err("Invalid network ID"),
        }
    }
}

impl std::fmt::Display for NetworkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mainnet => write!(f, "mainnet"),
            Self::Testnet => write!(f, "testnet"),
            Self::Canary => write!(f, "canary"),
        }
    }
}

impl From<NetworkId> for u16 {
    fn from(id: NetworkId) -> Self {
        match id {
            NetworkId::Mainnet => <MainnetV0 as NetworkTrait>::ID,
            NetworkId::Testnet => <TestnetV0 as NetworkTrait>::ID,
            NetworkId::Canary => <CanaryV0 as NetworkTrait>::ID,
        }
    }
}

impl NetworkId {
    pub fn from_network<N: NetworkTrait>() -> Self {
        match N::ID {
            <MainnetV0 as NetworkTrait>::ID => Self::Mainnet,
            <TestnetV0 as NetworkTrait>::ID => Self::Testnet,
            <CanaryV0 as NetworkTrait>::ID => Self::Canary,
            _ => unreachable!(),
        }
    }
}

pub trait Network: NetworkTrait {
    type Circuit: Aleo<Network = Self>;
    fn process<'a>() -> &'a Process<Self>;
}

macro_rules! network_to_circuit {
    ($net_name:ident = $circuit_name:ident) => {
        impl Network for $net_name {
            type Circuit = $circuit_name;

            fn process<'a>() -> &'a Process<$net_name> {
                static PROCESS: OnceLock<Process<$net_name>> = OnceLock::new();
                PROCESS.get_or_init(|| Process::load().unwrap())
            }
        }
    };
}

network_to_circuit!(MainnetV0 = AleoV0);
network_to_circuit!(TestnetV0 = AleoTestnetV0);
network_to_circuit!(CanaryV0 = AleoCanaryV0);

// The db.
pub type Db<N> = ConsensusDB<N>;

// Ledger types.
pub type DbLedger<N> = Ledger<N, Db<N>>;
pub type MemoryLedger<N> = Ledger<N, ConsensusMemory<N>>;

// The vm types.
pub type MemVM<N> = snarkvm::synthesizer::VM<N, ConsensusMemory<N>>;
pub type VM<N> = snarkvm::synthesizer::VM<N, Db<N>>;

// Tx types.
pub type TransactionID<N> = <N as NetworkTrait>::TransactionID;
pub type Transaction<N> = snarkvm::ledger::Transaction<N>;

// Account types.
pub type Account<N> = snarkos_account::Account<N>;

// User types.
pub type Address<N> = snarkvm::console::types::Address<N>;
pub type PrivateKey<N> = snarkvm::console::account::PrivateKey<N>;
pub type ViewKey<N> = snarkvm::console::account::ViewKey<N>;

// Value types.
// Text types.
pub type Ciphertext<N> = snarkvm::console::program::Ciphertext<N>;
pub type Plaintext<N> = snarkvm::console::program::Plaintext<N>;

// Record types.
pub type CTRecord<N> = snarkvm::console::program::Record<N, Ciphertext<N>>;
pub type PTRecord<N> = snarkvm::console::program::Record<N, Plaintext<N>>;

// Other types.
pub type Value<N> = snarkvm::console::program::Value<N>;
pub type Literal<N> = snarkvm::console::program::Literal<N>;

// Program types.
pub type Authorization<N> = snarkvm::synthesizer::Authorization<N>;
pub type Block<N> = snarkvm::ledger::Block<N>;
pub type Committee<N> = snarkvm::ledger::committee::Committee<N>;

pub fn gen_private_key<N: Network>() -> anyhow::Result<PrivateKey<N>> {
    let mut rng = ChaChaRng::from_entropy();
    PrivateKey::new(&mut rng)
}
