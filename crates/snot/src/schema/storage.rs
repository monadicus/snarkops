use std::{
    ops::Deref,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use anyhow::ensure;
use serde::{de::Visitor, Deserialize, Deserializer, Serialize};
use tokio::process::Command;
use tracing::warn;

use crate::state::GlobalState;

/// A storage document. Explains how storage for a test should be set up.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub id: FilenameString,
    pub name: String,
    pub description: Option<String>,
    /// Prefer using existing storage instead of generating new stuff.
    pub prefer_existing: bool,
    pub generate: Option<StorageGeneration>,
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

#[derive(Debug, Clone, Serialize)]
pub struct FilenameString(String);

struct FilenameStringVisitor;

impl<'de> Visitor<'de> for FilenameStringVisitor {
    type Value = FilenameString;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string that can be used as a filename")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v.contains('/') {
            Err(E::custom("filename string cannot have a path separator"))
        } else if v == "." || v == ".." {
            Err(E::custom("filename string cannot be relative"))
        } else {
            Ok(FilenameString(String::from(v)))
        }
    }
}

impl<'de> Deserialize<'de> for FilenameString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(FilenameStringVisitor)
    }
}

impl Deref for FilenameString {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<FilenameString> for String {
    fn from(value: FilenameString) -> Self {
        value.0
    }
}

impl Document {
    pub async fn prepare(self, state: &GlobalState) -> anyhow::Result<()> {
        static STORAGE_ID_INT: AtomicUsize = AtomicUsize::new(0);

        let id = String::from(self.id);

        // ensure this ID isn't already prepared
        if state.storage.read().await.contains_right(&id) {
            // TODO: we probably don't want to warn here. instead, it would be nice to
            // hash/checksum the storage to compare it with the conflicting storage
            warn!("a storage with the id {id} has already been prepared");
        }

        let mut base = state.cli.path.join("storage");
        base.push(&id);

        let exists = matches!(tokio::fs::try_exists(&base).await, Ok(true));

        // TODO: respect self.prefer_existing

        match self.generate {
            // generate the block and ledger if we have generation params
            Some(mut generation) => 'generate: {
                // warn if an existing block/ledger already exists
                if exists {
                    // TODO: is this the behavior we want?
                    warn!("the specified storage ID {id} already exists, using that one instead");
                    break 'generate;
                }

                generation.genesis = snarkos_aot::genesis::Genesis {
                    output: base.join("genesis.block"),
                    ledger: None,
                    additional_accounts_output: Some(base.join("accounts.json")),
                    committee_output: Some(base.join("committee.json")),
                    ..generation.genesis
                };

                tokio::task::spawn_blocking(move || generation.genesis.parse()).await??;

                // TODO: transactions
            }

            // no generation params passed
            None => {
                // assert that an existing block and ledger exists
                ensure!(exists, "the specified storage ID {id} doesn't exist, and no generation params were specified");
            }
        }

        // tar the ledger so that it can be served to agents
        // the genesis block is not compressed because it is already binary and might
        // not be served independently
        let ledger_exists = matches!(tokio::fs::try_exists(base.join("ledger")).await, Ok(true));
        let ledger_tar_exists = matches!(
            tokio::fs::try_exists(base.join("ledger.tar.gz")).await,
            Ok(true)
        );

        if ledger_exists && !ledger_tar_exists {
            let mut child = Command::new("tar")
                .current_dir(&base)
                .arg("-czf")
                .arg("ledger.tar.gz")
                .arg("ledger/")
                .kill_on_drop(true)
                .spawn()?;

            if !child.wait().await.map(|s| s.success()).unwrap_or(false) {
                warn!("failed to compress ledger");
            }
        }

        // add the prepared storage to the storage map
        let mut storage_lock = state.storage.write().await;
        let int_id = STORAGE_ID_INT.fetch_add(1, Ordering::Relaxed);
        storage_lock.insert(int_id, id.to_owned());

        Ok(())
    }
}
