use std::{fs, io::Write, os::unix::fs::PermissionsExt, path::PathBuf};

use checkpoint::CheckpointManager;
use futures_util::StreamExt;
use indexmap::IndexMap;
use rand::seq::IteratorRandom;
use sha2::{Digest, Sha256};
use snops_common::{
    api::{CheckpointMeta, StorageInfo},
    binaries::{BinaryEntry, BinarySource},
    key_source::KeySource,
    state::{InternedId, KeyState, NetworkId, StorageId},
};
use tracing::{info, trace};

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
            retention_policy: self.checkpoints.as_ref().map(|c| c.policy().clone()),
            checkpoints,
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
