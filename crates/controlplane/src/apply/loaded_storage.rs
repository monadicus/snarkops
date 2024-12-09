use std::{
    fs, io::Write, ops::Deref, os::unix::fs::PermissionsExt, path::PathBuf, process::Stdio,
    sync::Arc,
};

use futures_util::StreamExt;
use indexmap::IndexMap;
use rand::seq::IteratorRandom;
use sha2::{Digest, Sha256};
use snops_checkpoint::RetentionPolicy;
use snops_common::{
    aot_cmds::error::CommandError,
    api::StorageInfo,
    binaries::{BinaryEntry, BinarySource},
    constant::{SNARKOS_GENESIS_FILE, VERSION_FILE},
    key_source::{KeySource, ACCOUNTS_KEY_ID},
    schema::storage::{
        GenesisBalances, GenesisCommissions, GenesisGeneration, StorageDocument, StorageGeneration,
        DEFAULT_AOT_BINARY, STORAGE_DIR,
    },
    state::{InternedId, KeyState, NetworkId, StorageId},
};
use tokio::process::Command;
use tracing::{error, info, trace, warn};

use super::error::SchemaError;
use crate::{
    apply::{
        error::StorageError,
        storage_helpers::{
            get_version_from_path, pick_account_addr, pick_additional_addr, pick_commitee_addr,
            read_to_addrs,
        },
    },
    cli::Cli,
    persist::PersistStorage,
    state::GlobalState,
};

// IndexMap<addr, private_key>
pub type AleoAddrMap = IndexMap<String, String>;

#[derive(Debug, Clone)]
pub struct LoadedStorage {
    /// Storage ID
    pub id: StorageId,
    /// Network ID
    pub network: NetworkId,
    /// Version counter for this storage - incrementing will invalidate old
    /// saved ledgers
    pub version: u16,
    /// committee lookup
    pub committee: AleoAddrMap,
    /// other accounts files lookup
    pub accounts: IndexMap<InternedId, AleoAddrMap>,
    /// storage of checkpoints
    pub retention_policy: Option<RetentionPolicy>,
    /// whether agents using this storage should persist it
    pub persist: bool,
    /// whether to use the network's native genesis block
    pub native_genesis: bool,
    /// binaries available for this storage
    pub binaries: IndexMap<InternedId, BinaryEntry>,
}

impl LoadedStorage {
    pub async fn from_doc(
        doc: StorageDocument,
        state: &GlobalState,
        network: NetworkId,
    ) -> Result<Arc<LoadedStorage>, SchemaError> {
        let id = doc.id;

        // add the prepared storage to the storage map

        if state.storage.contains_key(&(network, id)) {
            // TODO: we probably don't want to warn here. instead, it would be nice to
            // hash/checksum the storage to compare it with the conflicting storage
            warn!("a storage with the id {id} has already been prepared");
        }

        let base = state.storage_path(network, id);
        let version_file = base.join(VERSION_FILE);

        let mut native_genesis = false;

        // TODO: The dir can be made by a previous run and the aot stuff can fail
        // i.e an empty/incomplete directory can exist and we should check those
        let mut exists = matches!(tokio::fs::try_exists(&base).await, Ok(true));

        // warn if an existing block/ledger already exists
        if exists {
            warn!("The specified storage ID {id} already exists");
        }

        let old_version = get_version_from_path(&version_file).await?;

        info!(
            "Storage {id} has version {old_version:?}. incoming version is {}",
            doc.regen
        );

        // wipe old storage when the version changes
        if old_version != Some(doc.regen) && exists {
            info!("Storage {id} version changed, removing old storage");
            tokio::fs::remove_dir_all(&base)
                .await
                .map_err(|e| StorageError::RemoveStorage(version_file.clone(), e))?;
            exists = false;
        }

        // gather the binaries
        let mut binaries = IndexMap::default();
        for (id, v) in doc.binaries {
            let mut entry =
                BinaryEntry::try_from(v).map_err(|e| StorageError::BinaryParse(id, e))?;
            if let BinarySource::Path(p) = &mut entry.source {
                if !p.exists() {
                    return Err(StorageError::BinaryFileMissing(id, p.clone()).into());
                }
                // canonicalize the path
                if let Ok(canon) = p.canonicalize() {
                    trace!(
                        "resolved binary relative path from {} to {}",
                        p.display(),
                        canon.display()
                    );
                    *p = canon
                }
            }
            info!("Resolved binary {id}: {entry}");
            binaries.insert(id, entry);
        }

        // resolve the default aot bin for this storage
        let aot_bin = LoadedStorage::resolve_binary_from_map(
            id,
            network,
            &binaries,
            state,
            InternedId::default(),
        )
        .await?;

        tokio::fs::create_dir_all(&base)
            .await
            .map_err(|e| StorageError::GenerateStorage(id, e))?;

        // generate the block and ledger if we have generation params
        if let (Some(generation), false) = (doc.generate.as_ref(), exists) {
            tracing::debug!("Generating storage for {id}");
            // generate the genesis block using the aot cli
            let output = base.join(SNARKOS_GENESIS_FILE);

            match (doc.connect, generation.genesis.as_ref()) {
                (None, None) => {
                    native_genesis = true;
                    info!("{id}: using network native genesis")
                }
                (Some(ref url), _) => {
                    // downloaded genesis block is not native
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
                (None, Some(genesis)) => {
                    // generated genesis block is not native
                    let mut command = Command::new(&aot_bin);
                    command
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit())
                        .env("NETWORK", network.to_string())
                        .arg("genesis")
                        .arg("--output")
                        .arg(&output);

                    // conditional seed flag
                    if let Some(seed) = genesis.seed {
                        command.arg("--seed").arg(seed.to_string());
                    }

                    // conditional genesis key flag
                    if let Some(private_key) = &genesis.private_key {
                        command.arg("--genesis-key").arg(private_key);
                    };

                    // generate committee based on the generation params
                    match &genesis.balances {
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

                    // generate committee commissions based on the generation params
                    match &genesis.commissions {
                        GenesisCommissions::Generated { bonded_commission } => {
                            if let Some(bonded_commission) = bonded_commission {
                                command
                                    .arg("--bonded-balance")
                                    .arg(bonded_commission.to_string());
                            }
                        }
                        GenesisCommissions::Defined { bonded_commissions } => {
                            command
                                .arg("--bonded-commissions")
                                .arg(serde_json::to_string(&bonded_commissions).unwrap());
                        }
                    }

                    if let Some(withdrawal) = &genesis.bonded_withdrawal {
                        command
                            .arg("--bonded-withdrawal")
                            .arg(serde_json::to_string(withdrawal).unwrap());
                    }

                    // conditionally add additional accounts
                    if let Some(additional_accounts) = genesis.additional_accounts {
                        command
                            .arg("--additional-accounts")
                            .arg(additional_accounts.to_string())
                            .arg("--additional-accounts-output")
                            .arg(base.join("accounts.json"));
                    }

                    if let Some(balance) = genesis.additional_accounts_balance {
                        command
                            .arg("--additional-accounts-balance")
                            .arg(balance.to_string());
                    }

                    info!("Generating genesis for {id} with command: {command:?}");

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

                    // ensure the genesis block was generated
                    tokio::fs::try_exists(&output)
                        .await
                        .map_err(|e| StorageError::FailedToGenGenesis(id, e))?;
                }
            }
        }

        let mut accounts = IndexMap::new();
        accounts.insert(
            *ACCOUNTS_KEY_ID,
            read_to_addrs(pick_additional_addr, &base.join("accounts.json")).await?,
        );

        if let Some(generation) = &doc.generate {
            for (name, account) in &generation.accounts {
                let path = base.join(format!("{}.json", name));

                if !path.exists() {
                    info!("generating accounts for {name}");

                    let mut command = Command::new(&aot_bin);
                    command
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit())
                        .env("NETWORK", network.to_string())
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
        tokio::fs::write(&version_file, doc.regen.to_string())
            .await
            .map_err(|e| StorageError::WriteVersion(version_file.clone(), e))?;

        let committee_file = base.join("committee.json");

        // if the committee was specified in the generation params, use that
        if let (
            Some(StorageGeneration {
                genesis:
                    Some(GenesisGeneration {
                        private_key,
                        balances: GenesisBalances::Defined { bonded_balances },
                        ..
                    }),
                ..
            }),
            false,
        ) = (doc.generate.as_ref(), committee_file.exists())
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
            version: doc.regen,
            id,
            network,
            committee,
            accounts,
            retention_policy: doc.retention_policy,
            persist: doc.persist,
            native_genesis,
            binaries,
        });
        if let Err(e) = state
            .db
            .storage
            .save(&(network, id), &PersistStorage::from(storage.deref()))
        {
            error!("failed to save storage meta: {e}");
        }
        state.storage.insert((network, id), storage.clone());

        Ok(storage)
    }

    pub fn lookup_keysource_pk(&self, key: &KeySource) -> KeyState {
        match key {
            KeySource::Local => KeyState::Local,
            KeySource::PrivateKeyLiteral(pk) => KeyState::Literal(pk.clone()),
            KeySource::PublicKeyLiteral(_) => KeyState::None,
            KeySource::ProgramLiteral(_) => KeyState::None,
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
            KeySource::PrivateKeyLiteral(_) => KeyState::None,
            KeySource::PublicKeyLiteral(addr) => KeyState::Literal(addr.clone()),
            KeySource::ProgramLiteral(addr) => KeyState::Literal(addr.clone()),
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
            KeySource::PrivateKeyLiteral(pk) => KeyState::Literal(pk.clone()),
            KeySource::PublicKeyLiteral(_) => KeyState::None,
            KeySource::ProgramLiteral(_) => KeyState::None,
            KeySource::Committee(Some(i)) => self
                .committee
                .get_index(*i)
                .map(|(_, pk)| pk.clone())
                .into(),
            KeySource::Committee(None) => self
                .committee
                .values()
                .choose(&mut rand::thread_rng())
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
                .and_then(|a| a.values().choose(&mut rand::thread_rng()).cloned())
                .into(),
        }
    }

    pub fn sample_keysource_addr(&self, key: &KeySource) -> KeyState {
        match key {
            KeySource::Local => KeyState::Local,
            KeySource::PrivateKeyLiteral(_) => KeyState::None,
            KeySource::PublicKeyLiteral(addr) => KeyState::Literal(addr.clone()),
            KeySource::ProgramLiteral(addr) => KeyState::Literal(addr.clone()),
            KeySource::Committee(Some(i)) => self
                .committee
                .get_index(*i)
                .map(|(addr, _)| addr.clone())
                .into(),
            KeySource::Committee(None) => self
                .committee
                .keys()
                .choose(&mut rand::thread_rng())
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
                .and_then(|a| a.keys().choose(&mut rand::thread_rng()).cloned())
                .into(),
        }
    }

    pub fn info(&self) -> StorageInfo {
        let mut binaries: IndexMap<_, _> = self
            .binaries
            .iter()
            .map(|(k, v)| (*k, v.with_api_path(self.network, self.id, *k)))
            .collect();

        // insert the default binary source information (so agents have a way to compare
        // shasums and file size)
        binaries
            .entry(InternedId::default())
            .or_insert(DEFAULT_AOT_BINARY.with_api_path(
                self.network,
                self.id,
                InternedId::default(),
            ));

        StorageInfo {
            id: self.id,
            version: self.version,
            retention_policy: self.retention_policy.clone(),
            persist: self.persist,
            native_genesis: self.native_genesis,
            binaries,
        }
    }

    pub fn path(&self, state: &GlobalState) -> PathBuf {
        self.path_cli(&state.cli)
    }

    pub fn path_cli(&self, cli: &Cli) -> PathBuf {
        let mut path = cli.path.join(STORAGE_DIR);
        path.push(self.network.to_string());
        path.push(self.id.to_string());
        path
    }

    /// Resolve the default binary for this storage
    pub async fn resolve_default_binary(
        &self,
        state: &GlobalState,
    ) -> Result<PathBuf, StorageError> {
        self.resolve_binary(state, InternedId::default()).await
    }

    /// Resolve the compute binary for this storage
    pub async fn resolve_compute_binary(
        &self,
        state: &GlobalState,
    ) -> Result<PathBuf, StorageError> {
        self.resolve_binary(state, InternedId::compute_id()).await
    }

    /// Resolve (find and download) a binary for this storage by id
    pub async fn resolve_binary(
        &self,
        state: &GlobalState,
        id: InternedId,
    ) -> Result<PathBuf, StorageError> {
        Self::resolve_binary_from_map(self.id, self.network, &self.binaries, state, id).await
    }

    /// Resolve a binary entry for this storage by id
    pub fn resolve_binary_entry(
        &self,
        id: InternedId,
    ) -> Result<(InternedId, &BinaryEntry), StorageError> {
        Self::resolve_binary_entry_from_map(self.id, &self.binaries, id)
    }

    pub fn resolve_binary_entry_from_map(
        storage_id: InternedId,
        binaries: &IndexMap<InternedId, BinaryEntry>,
        mut id: InternedId,
    ) -> Result<(InternedId, &BinaryEntry), StorageError> {
        let compute_id = InternedId::compute_id();

        // if the binary id is "compute" and there is no "compute" binary override in
        // the map, then we should use the default binary
        if id == compute_id && !binaries.contains_key(&compute_id) {
            id = InternedId::default();
        }

        // if the binary id is the default binary id and there is no default binary
        // override in the map,
        if id == InternedId::default() && !binaries.contains_key(&InternedId::default()) {
            // then we should use the default AOT binary
            return Ok((id, &DEFAULT_AOT_BINARY));
        }

        let bin = binaries
            .get(&id)
            .ok_or(StorageError::BinaryDoesNotExist(id, storage_id))?;

        Ok((id, bin))
    }

    pub async fn resolve_binary_from_map(
        storage_id: InternedId,
        network: NetworkId,
        binaries: &IndexMap<InternedId, BinaryEntry>,
        state: &GlobalState,
        id: InternedId,
    ) -> Result<PathBuf, StorageError> {
        let (id, bin) = Self::resolve_binary_entry_from_map(storage_id, binaries, id)?;

        let id_str: &str = id.as_ref();

        let remote_url = match bin.source.clone() {
            // if the binary is a relative path, then we should use the path as is
            // rather than downloading it
            BinarySource::Path(path) => return Ok(path.clone()),
            BinarySource::Url(url) => url,
        };

        // derive the path to the binary
        let mut download_path = state.cli.path.join(STORAGE_DIR);
        download_path.push(network.to_string());
        download_path.push(storage_id.to_string());
        download_path.push("binaries");
        download_path.push(id_str);

        // if the file already exists, ensure that it is the correct size and sha256
        if download_path.exists() {
            let perms = download_path
                .metadata()
                .map_err(|e| StorageError::PermissionError(download_path.clone(), e))?
                .permissions();
            if perms.mode() != 0o755 {
                std::fs::set_permissions(&download_path, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| StorageError::PermissionError(download_path.clone(), e))?;
            }

            match bin.check_file_sha256(&download_path) {
                Ok(None) => {}
                Ok(Some(sha256)) => {
                    return Err(StorageError::BinarySha256Mismatch(
                        storage_id,
                        download_path,
                        bin.sha256.clone().unwrap_or_default(),
                        sha256,
                    ));
                }
                Err(e) => {
                    return Err(StorageError::BinaryCheckFailed(
                        storage_id,
                        download_path,
                        e.to_string(),
                    ));
                }
            }

            match bin.check_file_size(&download_path) {
                // file is okay :)
                Ok(None) => {}
                Ok(Some(size)) => {
                    return Err(StorageError::BinarySizeMismatch(
                        storage_id,
                        download_path,
                        bin.size.unwrap_or_default(),
                        size,
                    ));
                }
                Err(e) => {
                    return Err(StorageError::BinaryCheckFailed(
                        storage_id,
                        download_path,
                        e.to_string(),
                    ));
                }
            }

            return Ok(download_path);
        }

        let resp = reqwest::get(remote_url.clone())
            .await
            .map_err(|e| StorageError::FailedToFetchBinary(id, remote_url.clone(), e))?;

        if resp.status() != reqwest::StatusCode::OK {
            return Err(StorageError::FailedToFetchBinaryWithStatus(
                id,
                remote_url,
                resp.status(),
            ));
        }

        if let Some(parent) = download_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| StorageError::FailedToCreateBinaryFile(id, e))?;
        }

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&download_path)
            .map_err(|e| StorageError::FailedToCreateBinaryFile(id, e))?;

        let mut digest = Sha256::new();
        let mut stream = resp.bytes_stream();
        let mut size = 0u64;

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(chunk) => {
                    size += chunk.len() as u64;
                    file.write_all(&chunk)
                        .map_err(|e| StorageError::FailedToWriteBinaryFile(id, e))?;
                    digest.update(&chunk);
                }
                Err(e) => {
                    return Err(StorageError::FailedToFetchBinary(id, remote_url, e));
                }
            }
        }

        // check if the binary sha256 matches the expected sha256
        let sha256 = format!("{:x}", digest.finalize());
        if let Some(bin_sha256) = bin.sha256.as_ref() {
            if bin_sha256.to_lowercase() != sha256 {
                return Err(StorageError::BinarySha256Mismatch(
                    id,
                    download_path,
                    bin_sha256.clone(),
                    sha256,
                ));
            }
        }

        // check if the binary size matches the expected size
        if let Some(bin_size) = bin.size {
            if bin_size != size {
                return Err(StorageError::BinarySizeMismatch(
                    id,
                    download_path,
                    bin_size,
                    size,
                ));
            }
        }

        info!(
            "downloaded binary {storage_id}.{id_str} to {} ({size} bytes)",
            download_path.display()
        );
        trace!("binary {storage_id}.{id_str} has sha256 {sha256}");

        let perms = download_path
            .metadata()
            .map_err(|e| StorageError::PermissionError(download_path.clone(), e))?
            .permissions();
        if perms.mode() != 0o755 {
            std::fs::set_permissions(&download_path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| StorageError::PermissionError(download_path.clone(), e))?;
        }

        Ok(download_path)
    }
}
