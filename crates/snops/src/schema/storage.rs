use std::{
    collections::HashMap,
    ops::Deref,
    path::PathBuf,
    process::{ExitStatus, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use checkpoint::{CheckpointManager, RetentionPolicy};
use indexmap::IndexMap;
use lazy_static::lazy_static;
use serde::{
    de::{DeserializeOwned, Visitor},
    Deserialize, Deserializer, Serialize,
};
use snops_common::{
    api::{CheckpointMeta, StorageInfo},
    constant::{LEDGER_BASE_DIR, LEDGER_STORAGE_FILE, SNARKOS_GENESIS_FILE},
    state::KeyState,
};
use tokio::process::Command;
use tracing::{error, info, warn};

use super::{
    error::{SchemaError, StorageError},
    nodes::KeySource,
};
use crate::{error::CommandError, state::GlobalState};

/// A storage document. Explains how storage for a test should be set up.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Document {
    pub id: FilenameString,
    pub name: String,
    pub description: Option<String>,
    /// Prefer using existing storage instead of generating new stuff.
    #[serde(default)]
    pub prefer_existing: bool,
    /// Tell nodes not to re-download the storage data.
    #[serde(default)]
    pub persist: bool,
    #[serde(default)]
    pub generate: Option<StorageGeneration>,
    #[serde(default)]
    pub connect: Option<url::Url>,
    #[serde(default)]
    pub retention_policy: Option<RetentionPolicy>,
}

/// Data generation instructions.
#[derive(Deserialize, Debug, Clone)]
pub struct StorageGeneration {
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
            output: PathBuf::from(SNARKOS_GENESIS_FILE),
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
    /// storage of checkpoints
    pub checkpoints: Option<CheckpointManager>,
    /// whether agents using this storage should persist it
    pub persist: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LedgerGeneration {
    pub output: PathBuf,
}

impl Default for LedgerGeneration {
    fn default() -> Self {
        Self {
            output: PathBuf::from(LEDGER_BASE_DIR),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FilenameString(String);

impl<'de> Deserialize<'de> for FilenameString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
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

lazy_static! {
    pub static ref DEFAULT_AOT_BIN: PathBuf =
        std::env::var("AOT_BIN").map(PathBuf::from).unwrap_or(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/release/snarkos-aot"),
        );
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

        // TODO: The dir can be made by a previous run and the aot stuff can fail
        // i.e an empty/incomplete directory can exist and we should check those
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
                let output = base.join(&generation.genesis.output);

                match self.connect {
                    Some(ref url) => {
                        let err =
                            |e| StorageError::FailedToFetchGenesis(id.clone(), url.clone(), e);

                        // I think its ok to reuse this error here
                        // because it just turns a failing response into an error
                        // or failing to turn it into bytes
                        let res = reqwest::get(url.clone())
                            .await
                            .map_err(err)?
                            .error_for_status()
                            .map_err(err)?
                            .bytes()
                            .await
                            .map_err(err)?;

                        tokio::fs::write(&output, res)
                            .await
                            .map_err(|e| StorageError::FailedToWriteGenesis(id.clone(), e))?;
                    }
                    None => {
                        let res = Command::new(DEFAULT_AOT_BIN.clone())
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
                            .arg(base.join(LEDGER_BASE_DIR))
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

                tokio::fs::try_exists(&output)
                    .await
                    .map_err(|e| StorageError::FailedToGenGenesis(id.clone(), e))?;

                let res = Command::new("tar")
                    .current_dir(&base)
                    .arg("czf")
                    .arg(LEDGER_STORAGE_FILE) // TODO: move constants from client...
                    .arg(LEDGER_BASE_DIR)
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

                tokio::fs::try_exists(&base.join(LEDGER_STORAGE_FILE))
                    .await
                    .map_err(|e| StorageError::FailedToTarLedger(id.clone(), e))?;

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
        let ledger_exists = matches!(
            tokio::fs::try_exists(base.join(LEDGER_BASE_DIR)).await,
            Ok(true)
        );
        let ledger_tar_exists = matches!(
            tokio::fs::try_exists(base.join(LEDGER_STORAGE_FILE)).await,
            Ok(true)
        );

        if ledger_exists && !ledger_tar_exists {
            let mut child = Command::new("tar")
                .current_dir(&base)
                .arg("czf")
                .arg(LEDGER_STORAGE_FILE)
                .arg("-C") // tar the contents of the "ledger" directory
                .arg(LEDGER_BASE_DIR)
                .arg(".")
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| {
                    StorageError::Command(
                        CommandError::action("spawning", "tar ledger", e),
                        id.clone(),
                    )
                })?;

            if !child
                .wait()
                .await
                .as_ref()
                .map(ExitStatus::success)
                .unwrap_or(false)
            {
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

        let checkpoints = self
            .retention_policy
            .map(|policy| {
                CheckpointManager::load(base.join(LEDGER_BASE_DIR), policy)
                    .map_err(StorageError::CheckpointManager)
            })
            .transpose()?;

        if let Some(checkpoints) = &checkpoints {
            info!("checkpoint manager loaded {checkpoints}");
        } else {
            info!("storage loaded without a checkpoint manager");
        }

        let storage = Arc::new(LoadedStorage {
            id: id.to_owned(),
            path: base.clone(),
            committee: read_to_addrs(pick_commitee_addr, base.join("committee.json")).await?,
            accounts,
            checkpoints,
            persist: self.persist,
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

// TODO: function should also take storage id
// in case of error, the storage id can be used to provide more context
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

    pub fn info(&self) -> StorageInfo {
        let checkpoints = self
            .checkpoints
            .as_ref()
            .map(|c| {
                c.checkpoints()
                    .filter_map(|(c, path)| {
                        path.file_name()
                            .and_then(|s| s.to_str())
                            .map(|filename| CheckpointMeta {
                                filename: filename.to_string(),
                                height: c.block_height,
                                timestamp: c.timestamp,
                            })
                    })
                    .collect()
            })
            .unwrap_or_default();
        StorageInfo {
            id: self.id.clone(),
            retention_policy: self.checkpoints.as_ref().map(|c| c.policy().clone()),
            checkpoints,
            persist: self.persist,
        }
    }
}
