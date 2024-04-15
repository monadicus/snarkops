use std::{
    collections::HashMap,
    ops::Deref,
    path::PathBuf,
    process::{ExitStatus, Stdio},
    sync::Arc,
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
    constant::{LEDGER_BASE_DIR, LEDGER_STORAGE_FILE, SNARKOS_GENESIS_FILE, VERSION_FILE},
    state::{InternedId, KeyState, StorageId},
};
use tokio::process::Command;
use tracing::{error, info, warn};

use super::{
    error::{SchemaError, StorageError},
    nodes::{KeySource, ACCOUNTS_KEY_ID},
};
use crate::{
    cli::Cli,
    db::document::DbDocument,
    error::CommandError,
    state::{persist::PersistStorage, GlobalState},
};

pub const STORAGE_DIR: &str = "storage";

/// A storage document. Explains how storage for a test should be set up.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Document {
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
}

/// Data generation instructions.
#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct StorageGeneration {
    // TODO: individually validate arguments, or just pass them like this?
    #[serde(default)]
    pub genesis: GenesisGeneration,

    #[serde(default)]
    pub accounts: IndexMap<InternedId, Accounts>,

    #[serde(default)]
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Accounts {
    pub count: u16,
    #[serde(default)]
    pub seed: Option<u64>,
}

impl<'de> Deserialize<'de> for Accounts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AccountsVisitor;

        impl<'de> Visitor<'de> for AccountsVisitor {
            type Value = Accounts;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number or an object with a count and seed")
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Accounts {
                    count: v.min(u16::MAX as u64) as u16,
                    seed: None,
                })
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut count = None;
                let mut seed = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        "count" => {
                            if count.is_some() {
                                return Err(serde::de::Error::duplicate_field("count"));
                            }
                            count = Some(map.next_value()?);
                        }
                        "seed" => {
                            if seed.is_some() {
                                return Err(serde::de::Error::duplicate_field("seed"));
                            }
                            seed = Some(map.next_value()?);
                        }
                        _ => return Err(serde::de::Error::unknown_field(key, &["count", "seed"])),
                    }
                }

                Ok(Accounts {
                    count: count.ok_or_else(|| serde::de::Error::missing_field("count"))?,
                    seed,
                })
            }
        }

        deserializer.deserialize_any(AccountsVisitor)
    }
}

// TODO: I don't know what this type should look like
#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct Transaction {
    pub file: PathBuf,
    pub total: u64,
    pub amount: u64,
    pub sources: Vec<String>,
    pub destinations: Vec<String>,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GenesisGeneration {
    // TODO: bonded balances mode, seed, genesis_key
    pub private_key: Option<String>,
    pub seed: Option<u64>,
    pub additional_accounts: Option<u16>,
    pub additional_accounts_balance: Option<u64>,
    #[serde(flatten)]
    pub balances: GenesisBalances,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
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
        }
    }
}

// IndexMap<addr, private_key>
pub type AleoAddrMap = IndexMap<String, String>;

#[derive(Debug, Clone)]
pub struct LoadedStorage {
    /// Storage ID
    pub id: StorageId,
    /// Version counter for this storage - incrementing will invalidate old
    /// saved ledgers
    pub version: u16,
    /// committee lookup
    pub committee: AleoAddrMap,
    /// other accounts files lookup
    pub accounts: HashMap<InternedId, AleoAddrMap>,
    /// storage of checkpoints
    pub checkpoints: Option<CheckpointManager>,
    /// whether agents using this storage should persist it
    pub persist: bool,
}

lazy_static! {
    // TODO: support multiple architectures
    pub static ref DEFAULT_AOT_BIN: PathBuf =
        std::env::var("AOT_BIN").map(PathBuf::from).unwrap_or(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/release/snarkos-aot"),
        );
    pub static ref DEFAULT_AGENT_BIN: PathBuf =
        std::env::var("AGENT_BIN").map(PathBuf::from).unwrap_or(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/release/snops-agent"),
        );
}

impl Document {
    pub async fn prepare(self, state: &GlobalState) -> Result<Arc<LoadedStorage>, SchemaError> {
        let id = self.id;

        // todo: maybe update the loaded storage in global state if the hash
        // of the storage document is different I guess...
        // that might interfere with running tests, so I don't know

        // add the prepared storage to the storage map

        if state.storage.read().await.contains_key(&id) {
            // TODO: we probably don't want to warn here. instead, it would be nice to
            // hash/checksum the storage to compare it with the conflicting storage
            warn!("a storage with the id {id} has already been prepared");
        }

        let base = state.cli.path.join(STORAGE_DIR).join(id.to_string());
        let version_file = base.join(VERSION_FILE);

        // TODO: The dir can be made by a previous run and the aot stuff can fail
        // i.e an empty/incomplete directory can exist and we should check those
        let mut exists = matches!(tokio::fs::try_exists(&base).await, Ok(true));

        // wipe old storage when the version changes
        if get_version_from_path(&version_file).await? != Some(self.regen) && exists {
            info!("storage {id} version changed, removing old storage");
            tokio::fs::remove_dir_all(&base)
                .await
                .map_err(|e| StorageError::RemoveStorage(version_file.clone(), e))?;
            exists = false;
        }

        match self.generate {
            // generate the block and ledger if we have generation params
            Some(ref generation) => 'generate: {
                // warn if an existing block/ledger already exists
                if exists {
                    // TODO: is this the behavior we want?
                    warn!("the specified storage ID {id} already exists, using that one instead");
                    break 'generate;
                } else {
                    tracing::debug!("generating storage for {id}");
                    tokio::fs::create_dir_all(&base)
                        .await
                        .map_err(|e| StorageError::GenerateStorage(id, e))?;
                }

                // generate the genesis block using the aot cli
                let output = base.join(SNARKOS_GENESIS_FILE);

                match self.connect {
                    Some(ref url) => {
                        let err = |e| StorageError::FailedToFetchGenesis(id, url.clone(), e);

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
                            .map_err(|e| StorageError::FailedToWriteGenesis(id, e))?;
                    }
                    None => {
                        let mut command = Command::new(DEFAULT_AOT_BIN.clone());
                        command
                            .stdout(Stdio::inherit())
                            .stderr(Stdio::inherit())
                            .arg("genesis")
                            .arg("--output")
                            .arg(&output)
                            .arg("--ledger")
                            .arg(base.join(LEDGER_BASE_DIR));

                        // conditional seed flag
                        if let Some(seed) = generation.genesis.seed {
                            command.arg("--seed").arg(seed.to_string());
                        }

                        // conditional genesis key flag
                        if let Some(private_key) = &generation.genesis.private_key {
                            command.arg("--genesis-key").arg(private_key);
                        };

                        // generate committee based on the generation params
                        match &generation.genesis.balances {
                            GenesisBalances::Generated {
                                committee_size,
                                bonded_balance,
                            } => {
                                command
                                    .arg("--committee-output")
                                    .arg(base.join("committee.json"));

                                if let Some(committee_size) = committee_size {
                                    command
                                        .arg("--committee-size")
                                        .arg(committee_size.to_string());
                                }
                                if let Some(bonded_balance) = bonded_balance {
                                    command
                                        .arg("--bonded-balance")
                                        .arg(bonded_balance.to_string());
                                }
                            }
                            GenesisBalances::Defined { bonded_balances } => {
                                command
                                    .arg("--bonded-balances")
                                    .arg(serde_json::to_string(&bonded_balances).unwrap());
                            }
                        }

                        // conditionally add additional accounts
                        if let Some(additional_accounts) = generation.genesis.additional_accounts {
                            command
                                .arg("--additional-accounts")
                                .arg(additional_accounts.to_string())
                                .arg("--additional-accounts-output")
                                .arg(base.join("accounts.json"));
                        }

                        if let Some(balance) = generation.genesis.additional_accounts_balance {
                            command
                                .arg("--additional-accounts-balance")
                                .arg(balance.to_string());
                        }

                        info!("{command:?}");

                        let res = command
                            .spawn()
                            .map_err(|e| {
                                StorageError::Command(
                                    CommandError::action("spawning", "aot genesis", e),
                                    id,
                                )
                            })?
                            .wait()
                            .await
                            .map_err(|e| {
                                StorageError::Command(
                                    CommandError::action("waiting", "aot genesis", e),
                                    id,
                                )
                            })?;

                        if !res.success() {
                            warn!("failed to run genesis generation command...");
                        }
                    }
                }

                // ensure the genesis block was generated
                tokio::fs::try_exists(&output)
                    .await
                    .map_err(|e| StorageError::FailedToGenGenesis(id, e))?;

                // tar a ledger if it exists
                let res = Command::new("tar")
                    .current_dir(&base)
                    .arg("czf")
                    .arg(LEDGER_STORAGE_FILE) // TODO: move constants from client...
                    .arg(LEDGER_BASE_DIR)
                    .kill_on_drop(true)
                    .spawn()
                    .map_err(|e| {
                        StorageError::Command(CommandError::action("spawning", "tar ledger", e), id)
                    })?
                    .wait()
                    .await
                    .map_err(|e| {
                        StorageError::Command(CommandError::action("waiting", "tar ledger", e), id)
                    })?;

                if !res.success() {
                    warn!("error running tar command...");
                }

                tokio::fs::try_exists(&base.join(LEDGER_STORAGE_FILE))
                    .await
                    .map_err(|e| StorageError::FailedToTarLedger(id, e))?;

                // TODO: transactions
            }

            // no generation params passed
            None => {
                // assert that an existing block and ledger exists
                if exists {
                    Err(StorageError::NoGenerationParams(id))?;
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
                    StorageError::Command(CommandError::action("spawning", "tar ledger", e), id)
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
            *ACCOUNTS_KEY_ID,
            read_to_addrs(pick_additional_addr, &base.join("accounts.json")).await?,
        );

        if let Some(generation) = &self.generate {
            for (name, account) in &generation.accounts {
                let path = base.join(&format!("{}.json", name));

                if !path.exists() {
                    info!("generating accounts for {name}");

                    let mut command = Command::new(DEFAULT_AOT_BIN.clone());
                    command
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit())
                        .arg("accounts")
                        .arg(account.count.to_string())
                        .arg("--output")
                        .arg(&path);
                    if let Some(seed) = account.seed {
                        command.arg("--seed").arg(seed.to_string());
                    }

                    let res = command
                        .spawn()
                        .map_err(|e| {
                            StorageError::Command(
                                CommandError::action("spawning", "aot accounts", e),
                                id,
                            )
                        })?
                        .wait()
                        .await
                        .map_err(|e| {
                            StorageError::Command(
                                CommandError::action("waiting", "aot accounts", e),
                                id,
                            )
                        })?;

                    if !res.success() {
                        warn!("failed to run account generation command for {name}...");
                    }
                }

                accounts.insert(*name, read_to_addrs(pick_account_addr, &path).await?);
            }
        }

        // write the regen version to a "version" file
        tokio::fs::write(&version_file, self.regen.to_string())
            .await
            .map_err(|e| StorageError::WriteVersion(version_file.clone(), e))?;

        let checkpoints = self
            .retention_policy
            .map(|policy| {
                CheckpointManager::load(base.join(LEDGER_BASE_DIR), policy)
                    .map_err(StorageError::CheckpointManager)
            })
            .transpose()?;

        if let Some(checkpoints) = &checkpoints {
            info!("storage {id} checkpoint manager loaded {checkpoints}");
        } else {
            info!("storage {id} loaded without a checkpoint manager");
        }

        let committee_file = base.join("committee.json");

        // if the committee was specified in the generation params, use that
        if let (
            Some(StorageGeneration {
                genesis:
                    GenesisGeneration {
                        private_key,
                        balances: GenesisBalances::Defined { bonded_balances },
                        ..
                    },
                ..
            }),
            false,
        ) = (self.generate.as_ref(), committee_file.exists())
        {
            // TODO: should be possible to get committee from genesis blocks
            let mut balances: IndexMap<_, _> = bonded_balances
                .iter()
                .map(|(addr, bal)| (addr.clone(), (String::new(), *bal)))
                .collect();

            // derive the committee member 0's key
            if let (Some(key), true) = (private_key, !balances.is_empty()) {
                balances[0].0.clone_from(key)
            }

            // write balances to committee.json if if doesn't exist
            tokio::fs::write(&committee_file, serde_json::to_string(&balances).unwrap())
                .await
                .map_err(|e| StorageError::WriteCommittee(committee_file.clone(), e))?;
        };
        // otherwise read the committee from the committee.json file
        let committee = read_to_addrs(pick_commitee_addr, &committee_file).await?;

        let storage = Arc::new(LoadedStorage {
            version: self.regen,
            id,
            committee,
            accounts,
            checkpoints,
            persist: self.persist,
        });
        let mut storage_lock = state.storage.write().await;
        if let Err(e) = PersistStorage::from(storage.deref()).save(&state.db, id) {
            error!("failed to save storage meta: {e}");
        }
        storage_lock.insert(id.to_owned(), storage.clone());

        Ok(storage)
    }
}

pub fn pick_additional_addr(entry: (String, u64, Option<serde_json::Value>)) -> String {
    entry.0
}
pub fn pick_commitee_addr(entry: (String, u64)) -> String {
    entry.0
}
pub fn pick_account_addr(entry: String) -> String {
    entry
}

// TODO: function should also take storage id
// in case of error, the storage id can be used to provide more context
pub async fn read_to_addrs<T: DeserializeOwned>(
    f: impl Fn(T) -> String,
    file: &PathBuf,
) -> Result<AleoAddrMap, StorageError> {
    if !file.exists() {
        return Ok(Default::default());
    }

    let data = tokio::fs::read_to_string(file)
        .await
        .map_err(|e| StorageError::ReadBalances(file.clone(), e))?;
    let parsed: IndexMap<String, T> =
        serde_json::from_str(&data).map_err(|e| StorageError::ParseBalances(file.clone(), e))?;

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
            id: self.id,
            version: self.version,
            retention_policy: self.checkpoints.as_ref().map(|c| c.policy().clone()),
            checkpoints,
            persist: self.persist,
        }
    }

    pub fn path(&self, state: &GlobalState) -> PathBuf {
        self.path_cli(&state.cli)
    }

    pub fn path_cli(&self, cli: &Cli) -> PathBuf {
        let mut path = cli.path.join(STORAGE_DIR);
        path.push(self.id.to_string());
        path
    }
}

async fn get_version_from_path(path: &PathBuf) -> Result<Option<u16>, StorageError> {
    if !path.exists() {
        return Ok(None);
    }

    let data = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| StorageError::ReadVersion(path.clone(), e))?;

    Ok(data.parse().ok())
}
