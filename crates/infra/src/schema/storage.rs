use std::path::PathBuf;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// A storage document. Explains how storage for a test should be set up.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Document {
    pub name: String,
    pub description: Option<String>,
    pub storage: LedgerStorage,
    pub accounts: AccountSources,
    pub generate: Option<StorageGeneration>,
}

/// Ledger and genesis storage data.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LedgerStorage {
    pub genesis: PathBuf,
    pub ledger: PathBuf,
}

/// Where to pull account information from.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AccountSources {
    pub committee: PathBuf,
    pub accounts: PathBuf,
}

/// Data generation instructions.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StorageGeneration {
    // TODO: how is this different from `LedgerStorage`?
    pub path: PathBuf,

    // TODO: individually validate arguments, or just pass them like this?
    pub genesis: IndexMap<String, serde_yaml::Value>,
    pub ledger: IndexMap<String, serde_yaml::Value>,
    pub transactions: Vec<Transaction>,
}

// TODO: I don't know what this type should look like
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Transaction {
    pub file: PathBuf,
    pub total: u64,
    pub amount: u64,
    pub sources: Vec<String>,
    pub destinations: Vec<String>,
}
