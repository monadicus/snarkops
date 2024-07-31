use std::{io::Write, path::PathBuf};

use checkpoint::CheckpointManager;
use indexmap::IndexMap;
use rand::seq::IteratorRandom;
use snops_common::{
    api::{CheckpointMeta, StorageInfo},
    binaries::{BinaryEntry, BinarySource},
    key_source::KeySource,
    state::{InternedId, KeyState, NetworkId, StorageId},
};

use super::{DEFAULT_AOT_BINARY, STORAGE_DIR};
use crate::{cli::Cli, schema::error::StorageError, state::GlobalState};

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
    pub checkpoints: Option<CheckpointManager>,
    /// whether agents using this storage should persist it
    pub persist: bool,
    /// whether to use the network's native genesis block
    pub native_genesis: bool,
    /// binaries available for this storage
    pub binaries: IndexMap<InternedId, BinaryEntry>,
}

impl LoadedStorage {
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
            native_genesis: self.native_genesis,
            binaries: self
                .binaries
                .iter()
                .map(|(k, v)| (*k, v.with_api_path(self.network, self.id, *k)))
                .collect(),
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

        if download_path.exists() {
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

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&download_path)
            .map_err(|e| StorageError::FailedToCreateBinaryFile(id, e))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| StorageError::FailedToFetchBinary(id, remote_url, e))?;
        file.write_all(&bytes)
            .map_err(|e| StorageError::FailedToWriteBinaryFile(id, e))?;

        Ok(download_path)
    }
}
