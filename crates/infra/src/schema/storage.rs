use std::path::PathBuf;

use serde::Deserialize;

/// A storage document. Explains how storage for a test should be set up.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub name: String,
    pub description: Option<String>,
    pub storage: LedgerStorage,
    pub accounts: Option<AccountSources>,
    pub generate: Option<StorageGeneration>,
}

/// Ledger and genesis storage data.
#[derive(Deserialize, Debug, Clone)]
pub struct LedgerStorage {
    pub genesis: PathBuf,
    pub ledger: PathBuf,
}

/// Where to pull account information from.
#[derive(Deserialize, Debug, Clone)]
pub struct AccountSources {
    pub committee: PathBuf,
    pub accounts: PathBuf,
}

/// Data generation instructions.
#[derive(Deserialize, Debug, Clone)]
pub struct StorageGeneration {
    // TODO: how is this different from `LedgerStorage`?
    pub path: PathBuf,

    // TODO: individually validate arguments, or just pass them like this?
    pub genesis: snarkos_aot::genesis::Genesis,
    pub ledger: LedgerGeneration,

    #[serde(default)]
    pub transactions: Vec<Transaction>,
}

// TODO: I don't know what this type should look like
#[derive(Deserialize, Debug, Clone)]
pub struct Transaction {
    pub file: PathBuf,
    pub total: u64,
    pub amount: u64,
    pub sources: Vec<String>,
    pub destinations: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LedgerGeneration {
    pub output: PathBuf,
}
