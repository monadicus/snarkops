use std::{
    collections::HashMap,
    ops::Deref,
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use indexmap::IndexMap;
use serde::{
    de::{DeserializeOwned, Visitor},
    Deserialize, Deserializer, Serialize,
};
use snot_common::state::KeyState;
use tokio::process::Command;
use tracing::{error, warn};

use super::{
    error::{SchemaError, StorageError},
    nodes::KeySource,
};
use crate::{error::CommandError, state::GlobalState};

/// A storage document. Explains how storage for a test should be set up.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub id: FilenameString,
    pub name: String,
    pub description: Option<String>,
    /// Prefer using existing storage instead of generating new stuff.
    #[serde(default)]
    pub prefer_existing: bool,
    pub generate: Option<StorageGeneration>,
    pub connect: Option<url::Url>,
}

/// Data generation instructions.
#[derive(Deserialize, Debug, Clone)]
pub struct StorageGeneration {
    // TODO: how is this different from `LedgerStorage`?
    pub path: PathBuf,

    // TODO: individually validate arguments, or just pass them like this?
    #[serde(default)]
    pub genesis: GenesisGeneration,
    #[serde(default)]
    pub ledger: LedgerGeneration,

    #[serde(default)]
    pub accounts: Vec<Accounts>,

    #[serde(default)]
    pub transactions: Vec<Transaction>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Accounts {
    pub file: PathBuf,
    pub total: u64,
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
#[serde(rename_all = "kebab-case")]
pub struct GenesisGeneration {
    pub output: PathBuf,
    pub committee: usize,
    pub committee_balances: usize,
    pub additional_accounts: usize,
    pub additional_balances: usize,
}

impl Default for GenesisGeneration {
    fn default() -> Self {
        Self {
            output: PathBuf::from("genesis.block"),
            committee: 5,
            committee_balances: 10_000_000_000_000,
            additional_accounts: 5,
            additional_balances: 100_000_000_000,
        }
    }
}

// IndexMap<addr, private_key>
pub type AleoAddrMap = IndexMap<String, String>;

#[derive(Debug, Clone)]
pub struct LoadedStorage {
    /// Storage ID
    pub id: String,
    /// Path to storage data
    pub path: PathBuf,
    /// committee lookup
    pub committee: AleoAddrMap,
    /// other accounts files lookup
    pub accounts: HashMap<String, AleoAddrMap>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LedgerGeneration {
    pub output: PathBuf,
}

impl Default for LedgerGeneration {
    fn default() -> Self {
        Self {
            output: PathBuf::from("ledger"),
        }
    }
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
    pub async fn prepare(self, state: &GlobalState) -> Result<Arc<LoadedStorage>, SchemaError> {
        static STORAGE_ID_INT: AtomicUsize = AtomicUsize::new(0);

        let id = String::from(self.id.clone());

        // ensure this ID isn't already prepared
        if state.storage_ids.read().await.contains_right(&id) {
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
            Some(generation) => 'generate: {
                // warn if an existing block/ledger already exists
                if exists {
                    // TODO: is this the behavior we want?
                    warn!("the specified storage ID {id} already exists, using that one instead");
                    break 'generate;
                } else {
                    tracing::debug!("generating storage for {id}");
                    tokio::fs::create_dir_all(&base)
                        .await
                        .map_err(|e| StorageError::GenerateStorage(id.clone(), e))?;
                }

                // generate the genesis block using the aot cli
                let bin = std::env::var("AOT_BIN").map(PathBuf::from).unwrap_or(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../../target/release/snarkos-aot"),
                );
                let output = base.join(&generation.genesis.output);

                match self.connect {
                    Some(ref url) => {
                        // I think its ok to reuse this error here
                        // because it just turns a failing response into an error
                        // or failing to turn it into bytes
                        let res = reqwest::get(url.clone())
                            .await
                            .map_err(|e| {
                                StorageError::FailedToFetchGenesis(id.clone(), url.clone(), e)
                            })?
                            .error_for_status()
                            .map_err(|e| {
                                StorageError::FailedToFetchGenesis(id.clone(), url.clone(), e)
                            })?
                            .bytes()
                            .await
                            .map_err(|e| {
                                StorageError::FailedToFetchGenesis(id.clone(), url.clone(), e)
                            })?;

                        tokio::fs::write(&output, res)
                            .await
                            .map_err(|e| StorageError::FailedToWriteGenesis(id.clone(), e))?;
                    }
                    None => {
                        let res = Command::new(bin)
                            .stdout(Stdio::inherit())
                            .stderr(Stdio::inherit())
                            .arg("genesis")
                            .arg("--output")
                            .arg(&output)
                            .arg("--committee-size")
                            .arg(generation.genesis.committee.to_string())
                            .arg("--committee-output")
                            .arg(base.join("committee.json"))
                            .arg("--additional-accounts")
                            .arg(generation.genesis.additional_accounts.to_string())
                            .arg("--additional-accounts-output")
                            .arg(base.join("accounts.json"))
                            .arg("--ledger")
                            .arg(base.join("ledger"))
                            .spawn()
                            .map_err(|e| {
                                StorageError::Command(
                                    CommandError::action("spawning", "aot genesis", e),
                                    id.clone(),
                                )
                            })?
                            .wait()
                            .await
                            .map_err(|e| {
                                StorageError::Command(
                                    CommandError::action("waiting", "aot genesis", e),
                                    id.clone(),
                                )
                            })?;

                        if !res.success() {
                            warn!("failed to run genesis generation command...");
                        }
                    }
                }

                if let Err(e) = tokio::fs::try_exists(&output).await {
                    Err(StorageError::FailedToGenGenesis(id.clone(), e))?;
                }

                let res = Command::new("tar")
                    .current_dir(&base)
                    .arg("czf")
                    .arg("ledger.tar.gz") // TODO: move constants from client...
                    .arg("ledger")
                    .kill_on_drop(true)
                    .spawn()
                    .map_err(|e| {
                        StorageError::Command(
                            CommandError::action("spawning", "tar ledger", e),
                            id.clone(),
                        )
                    })?
                    .wait()
                    .await
                    .map_err(|e| {
                        StorageError::Command(
                            CommandError::action("waiting", "tar ledger", e),
                            id.clone(),
                        )
                    })?;

                if !res.success() {
                    warn!("error running tar command...");
                }

                if let Err(e) = tokio::fs::try_exists(&base.join("ledger.tar.gz")).await {
                    Err(StorageError::FailedToTarLedger(id.clone(), e))?;
                }

                // TODO: transactions
            }

            // no generation params passed
            None => {
                // assert that an existing block and ledger exists
                if exists {
                    Err(StorageError::NoGenerationParams(id.clone()))?;
                }
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
                .spawn()
                .map_err(|e| {
                    StorageError::Command(
                        CommandError::action("spawning", "tar ledger", e),
                        id.clone(),
                    )
                })?;

            if !child.wait().await.map(|s| s.success()).unwrap_or(false) {
                error!("failed to compress ledger");
            }
        }

        let mut accounts = HashMap::new();
        accounts.insert(
            "accounts".to_owned(),
            read_to_addrs(pick_additional_addr, base.join("accounts.json")).await?,
        );

        // todo: maybe update the loaded storage in global state if the hash
        // of the storage document is different I guess...
        // that might interfere with running tests, so I don't know

        // add the prepared storage to the storage map
        let mut storage_lock = state.storage_ids.write().await;
        let int_id = STORAGE_ID_INT.fetch_add(1, Ordering::Relaxed);
        storage_lock.insert(int_id, id.to_owned());

        let storage = Arc::new(LoadedStorage {
            id: id.to_owned(),
            path: base.clone(),
            committee: read_to_addrs(pick_commitee_addr, base.join("committee.json")).await?,
            accounts,
        });
        let mut storage_lock = state.storage.write().await;
        storage_lock.insert(int_id, storage.clone());

        Ok(storage)
    }
}

fn pick_additional_addr(entry: (String, u64, Option<serde_json::Value>)) -> String {
    entry.0
}
fn pick_commitee_addr(entry: (String, u64)) -> String {
    entry.0
}

async fn read_to_addrs<T: DeserializeOwned>(
    f: impl Fn(T) -> String,
    file: PathBuf,
) -> Result<AleoAddrMap, SchemaError> {
    let data = tokio::fs::read_to_string(&file)
        .await
        .map_err(|e| StorageError::ReadBalances(file.clone(), e))?;
    let parsed: IndexMap<String, T> =
        serde_json::from_str(&data).map_err(|e| StorageError::ParseBalances(file, e))?;

    Ok(parsed.into_iter().map(|(k, v)| (k, f(v))).collect())
}

impl LoadedStorage {
    pub fn lookup_keysource_pk(&self, key: &KeySource) -> KeyState {
        match key {
            KeySource::Local => KeyState::Local,
            KeySource::Literal(pk) => KeyState::Literal(pk.clone()),
            KeySource::Committee(Some(i)) => self
                .committee
                .get_index(*i)
                .map(|(_, pk)| pk.clone())
                .into(),
            KeySource::Committee(None) => KeyState::None,
            KeySource::Named(name, Some(i)) => self
                .accounts
                .get(name)
                .and_then(|a| a.get_index(*i).map(|(_, pk)| pk.clone()))
                .into(),
            KeySource::Named(_name, None) => KeyState::None,
        }
    }

    pub fn lookup_keysource_addr(&self, key: &KeySource) -> KeyState {
        match key {
            KeySource::Local => KeyState::Local,
            KeySource::Literal(addr) => KeyState::Literal(addr.clone()),
            KeySource::Committee(Some(i)) => self
                .committee
                .get_index(*i)
                .map(|(addr, _)| addr.clone())
                .into(),
            KeySource::Committee(None) => KeyState::None,
            KeySource::Named(name, Some(i)) => self
                .accounts
                .get(name)
                .and_then(|a| a.get_index(*i).map(|(addr, _)| addr.clone()))
                .into(),
            KeySource::Named(_name, None) => KeyState::None,
        }
    }

    pub fn sample_keysource_pk(&self, key: &KeySource) -> KeyState {
        match key {
            KeySource::Local => KeyState::Local,
            KeySource::Literal(pk) => KeyState::Literal(pk.clone()),
            KeySource::Committee(Some(i)) => self
                .committee
                .get_index(*i)
                .map(|(_, pk)| pk.clone())
                .into(),
            KeySource::Committee(None) => self
                .committee
                .values()
                .nth(rand::random::<usize>() % self.committee.len())
                .cloned()
                .into(),
            KeySource::Named(name, Some(i)) => self
                .accounts
                .get(name)
                .and_then(|a| a.get_index(*i).map(|(_, pk)| pk.clone()))
                .into(),
            KeySource::Named(name, None) => self
                .accounts
                .get(name)
                .and_then(|a| {
                    a.values()
                        .nth(rand::random::<usize>() % self.accounts.len())
                        .cloned()
                })
                .into(),
        }
    }

    pub fn sample_keysource_addr(&self, key: &KeySource) -> KeyState {
        match key {
            KeySource::Local => KeyState::Local,
            KeySource::Literal(addr) => KeyState::Literal(addr.clone()),
            KeySource::Committee(Some(i)) => self
                .committee
                .get_index(*i)
                .map(|(addr, _)| addr.clone())
                .into(),
            KeySource::Committee(None) => self
                .committee
                .keys()
                .nth(rand::random::<usize>() % self.committee.len())
                .cloned()
                .into(),
            KeySource::Named(name, Some(i)) => self
                .accounts
                .get(name)
                .and_then(|a| a.get_index(*i).map(|(addr, _)| addr.clone()))
                .into(),
            KeySource::Named(name, None) => self
                .accounts
                .get(name)
                .and_then(|a| {
                    a.keys()
                        .nth(rand::random::<usize>() % self.accounts.len())
                        .cloned()
                })
                .into(),
        }
    }
}
