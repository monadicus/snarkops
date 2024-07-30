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

use super::{DEFAULT_AOT_BIN, STORAGE_DIR};
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
                .map(|(k, v)| (*k, v.with_api_path(self.id, *k)))
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

    pub async fn resolve_default_binary(&self, state: &GlobalState) -> PathBuf {
        match self.resolve_binary_inner(state, &Default::default()).await {
            Ok(path) => path,
            Err(_) => DEFAULT_AOT_BIN.clone(),
        }
    }

    pub async fn resolve_compute_binary(&self, state: &GlobalState) -> PathBuf {
        match self
            .resolve_binary_inner(state, &InternedId::compute_id())
            .await
        {
            Ok(path) => path,
            Err(_) => self.resolve_default_binary(state).await,
        }
    }

    pub async fn resolve_binary(
        &self,
        state: &GlobalState,
        id: &InternedId,
    ) -> Result<PathBuf, StorageError> {
        if id == &InternedId::default() {
            Ok(self.resolve_default_binary(state).await)
        } else if id == &InternedId::compute_id() {
            Ok(self.resolve_compute_binary(state).await)
        } else {
            self.resolve_binary_inner(state, id).await
        }
    }

    async fn resolve_binary_inner(
        &self,
        state: &GlobalState,
        id: &InternedId,
    ) -> Result<PathBuf, StorageError> {
        let bin = self
            .binaries
            .get(id)
            .ok_or(StorageError::BinaryDoesNotExist(*id, self.id))?;

        let id_str: &str = id.as_ref();
        let path = match bin.source.clone() {
            BinarySource::Path(path) => return Ok(path.clone()),
            BinarySource::Url(url) => {
                let path = self.path(state).join("binaries").join(id_str);
                if !path.exists() {
                    let resp = reqwest::get(url.clone())
                        .await
                        .map_err(|e| StorageError::FailedToFetchBinary(*id, url.clone(), e))?;

                    if resp.status() != reqwest::StatusCode::OK {
                        return Err(StorageError::FailedToFetchBinaryWithStatus(
                            *id,
                            url,
                            resp.status(),
                        ));
                    }

                    let mut download = std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&path)
                        .map_err(|e| StorageError::FailedToCreateBinaryFile(*id, e))?;
                    download
                        .write_all(resp.bytes().await.expect("TODO").as_ref())
                        .map_err(|e| StorageError::FailedToCreateBinaryFile(*id, e))?;
                }

                path
            }
        };

        Ok(path)
    }
}
