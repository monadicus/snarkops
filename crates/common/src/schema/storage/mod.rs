use std::path::PathBuf;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use snops_checkpoint::RetentionPolicy;

use crate::state::{InternedId, StorageId};

mod accounts;
use accounts::*;
mod binaries;
pub use binaries::*;

pub const STORAGE_DIR: &str = "storage";

/// A storage document. Explains how storage for a test should be set up.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StorageDocument {
    pub id: StorageId,
    /// Regen version
    #[serde(default)]
    pub regen: u16,
    pub name: String,
    pub description: Option<String>,
    /// Tell nodes not to re-download the storage data.
    #[serde(default)]
    pub persist: bool,
    #[serde(default)]
    pub generate: Option<StorageGeneration>,
    #[serde(default)]
    pub connect: Option<url::Url>,
    #[serde(default)]
    pub retention_policy: Option<RetentionPolicy>,
    /// The binaries list for this storage is used to determine which binaries
    /// are used by the agents.
    /// Overriding `default` will replace the node's default binary rather than
    /// using snops' own default aot binary.
    /// Overriding `compute` will replace the node's default binary only for
    /// compute
    #[serde(default)]
    pub binaries: IndexMap<InternedId, BinaryEntryDoc>,
}

/// Data generation instructions.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageGeneration {
    #[serde(default)]
    pub genesis: Option<GenesisGeneration>,

    #[serde(default)]
    pub accounts: IndexMap<InternedId, Accounts>,

    #[serde(default)]
    pub transactions: Vec<Transaction>,
}

// TODO: Convert this into a struct similar to the execute action, then use
// compute agents to assemble these on the fly
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Transaction {
    pub file: PathBuf,
    pub total: u64,
    pub amount: u64,
    pub sources: Vec<String>,
    pub destinations: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GenesisGeneration {
    pub private_key: Option<String>,
    pub seed: Option<u64>,
    pub additional_accounts: Option<u16>,
    pub additional_accounts_balance: Option<u64>,
    #[serde(flatten)]
    pub balances: GenesisBalances,
    #[serde(flatten)]
    pub commissions: GenesisCommissions,
    pub bonded_withdrawal: Option<IndexMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GenesisBalances {
    #[serde(rename_all = "kebab-case")]
    Defined {
        bonded_balances: IndexMap<String, u64>,
    },
    #[serde(rename_all = "kebab-case")]
    Generated {
        committee_size: Option<u16>,
        bonded_balance: Option<u64>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GenesisCommissions {
    #[serde(rename_all = "kebab-case")]
    Defined {
        bonded_commissions: IndexMap<String, u8>,
    },
    #[serde(rename_all = "kebab-case")]
    Generated { bonded_commission: Option<u8> },
}

impl Default for GenesisGeneration {
    fn default() -> Self {
        Self {
            seed: None,
            private_key: None,
            additional_accounts: None,
            additional_accounts_balance: None,
            balances: GenesisBalances::Generated {
                committee_size: None,
                bonded_balance: None,
            },
            commissions: GenesisCommissions::Generated {
                bonded_commission: None,
            },
            bonded_withdrawal: None,
        }
    }
}
