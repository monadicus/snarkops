use std::{
    fs::Permissions, os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc, time::Duration,
};

use chrono::{DateTime, TimeDelta, Utc};
use snops_checkpoint::CheckpointManager;
use snops_common::{
    api::EnvInfo,
    binaries::{BinaryEntry, BinarySource},
    constant::{
        LEDGER_BASE_DIR, LEDGER_PERSIST_DIR, LEDGER_STORAGE_FILE, SNARKOS_FILE,
        SNARKOS_GENESIS_FILE, VERSION_FILE,
    },
    rpc::error::{ReconcileError, ReconcileError2},
    state::{HeightRequest, InternedId, NetworkId, StorageId, TransferId, TransferStatusUpdate},
};
use tokio::process::Command;
use tracing::{debug, error, info, trace};
use url::Url;

use super::{checkpoint, Reconcile, ReconcileCondition, ReconcileStatus};
use crate::{
    api::{self, download_file, should_download_file},
    state::GlobalState,
    transfers,
};

pub fn default_binary(info: &EnvInfo) -> BinaryEntry {
    BinaryEntry {
        source: BinarySource::Path(PathBuf::from(format!(
            "/content/storage/{}/{}/binaries/default",
            info.network, info.storage.id
        ))),
        sha256: None,
        size: None,
    }
}

/// Ensure the correct binary is present for running snarkos
pub async fn ensure_correct_binary(
    binary_id: Option<InternedId>,
    state: &GlobalState,
    info: &EnvInfo,
) -> Result<(), ReconcileError> {
    let base_path = &state.cli.path;

    // TODO: store binary based on binary id
    // download the snarkOS binary
    api::check_binary(
        info.storage
            .binaries
            .get(&binary_id.unwrap_or_default())
            .unwrap_or(&default_binary(info)),
        &state.endpoint,
        &base_path.join(SNARKOS_FILE),
        state.transfer_tx(),
    )
    .await
    .map_err(|e| ReconcileError::BinaryAcquireError(e.to_string()))?;

    Ok(())
}

pub fn get_genesis_route(endpoint: &str, network: NetworkId, storage_id: StorageId) -> String {
    format!("{endpoint}/content/storage/{network}/{storage_id}/{SNARKOS_GENESIS_FILE}")
}

pub fn get_ledger_route(endpoint: &str, network: NetworkId, storage_id: StorageId) -> String {
    format!("{endpoint}/content/storage/{network}/{storage_id}/{LEDGER_STORAGE_FILE}")
}

/// Ensure all required files are present in the storage directory
pub async fn check_files(
    state: &GlobalState,
    info: &EnvInfo,
    height: &HeightRequest,
) -> Result<(), ReconcileError> {
    let base_path = &state.cli.path;
    let storage_id = &info.storage.id;
    let network = info.network;
    let storage_path = base_path
        .join("storage")
        .join(network.to_string())
        .join(storage_id.to_string());

    // create the directory containing the storage files
    tokio::fs::create_dir_all(&storage_path)
        .await
        .map_err(|_| ReconcileError::StorageSetupError("create storage directory".to_string()))?;

    let version_file = storage_path.join(VERSION_FILE);

    // wipe old storage when the version changes
    if get_version_from_path(&version_file).await? != Some(info.storage.version)
        && storage_path.exists()
    {
        let _ = tokio::fs::remove_dir_all(&storage_path).await;
    }

    std::fs::create_dir_all(&storage_path).map_err(|e| {
        error!("failed to create storage directory: {e}");
        ReconcileError::StorageSetupError("create storage directory".to_string())
    })?;

    let genesis_path = storage_path.join(SNARKOS_GENESIS_FILE);
    let genesis_url = get_genesis_route(&state.endpoint, network, *storage_id);
    let ledger_path = storage_path.join(LEDGER_STORAGE_FILE);
    let ledger_url = get_ledger_route(&state.endpoint, network, *storage_id);

    // skip genesis download for native genesis storage
    if !info.storage.native_genesis {
        // download the genesis block
        api::check_file(genesis_url, &genesis_path, state.transfer_tx())
            .await
            .map_err(|e| {
                error!("failed to download {SNARKOS_GENESIS_FILE} from the control plane: {e}");
                ReconcileError::StorageAcquireError(SNARKOS_GENESIS_FILE.to_owned())
            })?;
    }

    // don't download
    if height.reset() {
        info!("skipping ledger check due to 0 height request");
        return Ok(());
    }

    // download the ledger file
    api::check_file(ledger_url, &ledger_path, state.transfer_tx())
        .await
        .map_err(|e| {
            error!("failed to download {SNARKOS_GENESIS_FILE} from the control plane: {e}");
            ReconcileError::StorageAcquireError(LEDGER_STORAGE_FILE.to_owned())
        })?;

    // write the regen version to a "version" file
    tokio::fs::write(&version_file, info.storage.version.to_string())
        .await
        .map_err(|e| {
            error!("failed to write storage version: {e}");
            ReconcileError::StorageSetupError("write storage version".to_string())
        })?;

    Ok(())
}

/// This reconciler creates a directory if it does not exist
pub struct DirectoryReconciler(pub PathBuf);
impl Reconcile<(), ReconcileError2> for DirectoryReconciler {
    async fn reconcile(&mut self) -> Result<super::ReconcileStatus<()>, ReconcileError2> {
        std::fs::create_dir_all(&self.0)
            .map(ReconcileStatus::with)
            .map_err(|e| ReconcileError2::CreateDirectory(self.0.clone(), e.to_string()))
    }
}

/// The FileReconciler will download a file from a URL and place it in a local
/// directory. It will also check the file's size and sha256 hash if provided,
/// and set the file's permissions. If the file already exists, it will not be
/// downloaded again.
///
/// The reconciler will return true when the file is ready, and false when the
/// file cannot be obtained (offline controlplane).
pub struct FileReconciler {
    pub state: Arc<GlobalState>,
    pub src: Url,
    pub dst: PathBuf,
    pub offline: bool,
    pub tx_id: Option<TransferId>,
    pub permissions: Option<u32>,
    pub check_sha256: Option<String>,
    pub check_size: Option<u64>,
}
impl FileReconciler {
    pub fn new(state: Arc<GlobalState>, src: Url, dst: PathBuf) -> Self {
        Self {
            state,
            src,
            dst,
            offline: false,
            tx_id: None,
            permissions: None,
            check_sha256: None,
            check_size: None,
        }
    }

    pub fn with_offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    pub fn with_tx_id(mut self, tx_id: Option<TransferId>) -> Self {
        self.tx_id = tx_id;
        self
    }

    pub fn with_binary(mut self, binary: &BinaryEntry) -> Self {
        self.permissions = Some(0o755);
        self.check_sha256 = binary.sha256.clone();
        self.check_size = binary.size;
        self
    }

    pub fn check_and_set_mode(&self) -> Result<(), ReconcileError2> {
        // ensure the file has the correct permissions
        let Some(check_perms) = self.permissions else {
            return Ok(());
        };

        let perms = self
            .dst
            .metadata()
            .map_err(|e| ReconcileError2::FileStatError(self.dst.clone(), e.to_string()))?
            .permissions();

        if perms.mode() != check_perms {
            std::fs::set_permissions(&self.dst, std::fs::Permissions::from_mode(check_perms))
                .map_err(|e| {
                    ReconcileError2::FilePermissionError(self.dst.clone(), e.to_string())
                })?;
        }

        Ok(())
    }
}

impl Reconcile<bool, ReconcileError2> for FileReconciler {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<bool>, ReconcileError2> {
        let client = reqwest::Client::new();

        // Create a transfer id if one is not provided
        if self.tx_id.is_none() {
            self.tx_id = Some(transfers::next_id());
        }

        let tx_id = self.tx_id.unwrap();

        // transfer is pending
        match self.state.transfers.entry(tx_id) {
            dashmap::Entry::Occupied(occupied_entry) => {
                let entry = occupied_entry.get();

                if entry.is_pending() {
                    return Ok(ReconcileStatus::empty()
                        .add_condition(ReconcileCondition::PendingTransfer(
                            self.src.to_string(),
                            tx_id,
                        ))
                        .requeue_after(Duration::from_secs(1)));
                }

                if entry.is_interrupted() {
                    // if the failure is within the last 60 seconds, requeue
                    if Utc::now().signed_duration_since(entry.updated_at).abs()
                        < TimeDelta::seconds(60)
                    {
                        return Ok(ReconcileStatus::empty()
                            .add_condition(ReconcileCondition::InterruptedTransfer(
                                self.src.to_string(),
                                tx_id,
                                entry.interruption.clone().unwrap_or_default(),
                            ))
                            .requeue_after(Duration::from_secs(60)));
                    }

                    // if the failure is older than 60 seconds, remove the pending transfer and
                    // start over.
                    occupied_entry.remove();
                    return Ok(ReconcileStatus::empty()
                        .add_scope("file/interrupt/restart")
                        .requeue_after(Duration::from_secs(1)));
                }

                // entry is complete
            }
            dashmap::Entry::Vacant(_) => {}
        }

        let is_file_ready = !should_download_file(
            &client,
            self.src.as_str(),
            self.dst.as_path(),
            self.check_size,
            self.check_sha256.as_deref(),
            self.offline,
        )
        .await?;

        // Everything is good. Ensure file permissions
        if is_file_ready {
            self.check_and_set_mode()?;
            return Ok(ReconcileStatus::with(true));
        }

        // file does not exist and cannot be downloaded right now
        if !self.dst.exists() && self.offline {
            return Ok(ReconcileStatus::with(false));
        }

        let src = self.src.clone();
        let dst = self.dst.clone();
        let transfer_tx = self.state.transfer_tx.clone();

        // download the file
        let handle =
            tokio::spawn(
                async move { download_file(tx_id, &client, src, &dst, transfer_tx).await },
            )
            .abort_handle();

        // update the transfer with the handle (so it can be canceled if necessary)
        if let Err(e) = self
            .state
            .transfer_tx
            .send((tx_id, TransferStatusUpdate::Handle(handle)))
        {
            error!("failed to send transfer handle: {e}");
        }

        // transfer is pending - requeue after 1 second with the pending condition
        Ok(ReconcileStatus::empty()
            .add_condition(ReconcileCondition::PendingTransfer(
                self.src.to_string(),
                tx_id,
            ))
            .requeue_after(Duration::from_secs(1)))
    }
}

/// Untar the ledger file into the storage directory
pub async fn load_ledger(
    state: &GlobalState,
    info: &EnvInfo,
    height: &HeightRequest,
    is_new_env: bool,
) -> Result<bool, ReconcileError> {
    let base_path = &state.cli.path;
    let storage_id = &info.storage.id;
    let storage_path = base_path
        .join("storage")
        .join(info.network.to_string())
        .join(storage_id.to_string());

    // use a persisted directory for the untar when configured
    let (untar_base, untar_dir) = if info.storage.persist {
        info!("using persisted ledger for {storage_id}");
        (&storage_path, LEDGER_PERSIST_DIR)
    } else {
        info!("using fresh ledger for {storage_id}");
        (base_path, LEDGER_BASE_DIR)
    };

    let ledger_dir = untar_base.join(untar_dir);

    tokio::fs::create_dir_all(&ledger_dir.join(".aleo"))
        .await
        .map_err(|_| ReconcileError::StorageSetupError("create local aleo home".to_string()))?;

    // skip the top request if the persisted ledger already exists
    // this will prevent the ledger from getting wiped in the next step
    if info.storage.persist && height.is_top() && ledger_dir.exists() {
        info!("persisted ledger already exists for {storage_id}");
        return Ok(false);
    }

    let mut changed = false;

    // If there's a retention policy, load the checkpoint manager
    // this is so we can wipe all leftover checkpoints for non-persisted storage
    // after resets or new environments
    let mut manager = info
        .storage
        .retention_policy
        .clone()
        .map(|policy| {
            debug!("loading checkpoints from {untar_base:?}...");
            CheckpointManager::load(ledger_dir.clone(), policy).map_err(|e| {
                error!("failed to load checkpoints: {e}");
                ReconcileError::CheckpointLoadError
            })
        })
        .transpose()?;

    if let Some(manager) = &manager {
        info!("discovered checkpoints: {manager}");
    }

    // reload the storage if the height is reset or a new environment is created
    if height.reset() || is_new_env {
        // clean up old storage
        if ledger_dir.exists() {
            changed = true;
            if let Err(err) = tokio::fs::remove_dir_all(&ledger_dir).await {
                error!("failed to remove old ledger: {err}");
            }
        }

        // cleanup old checkpoints for non-persisted ledgers as they are
        // stored in a common location
        //
        // this also forces the rewind checkpoints to be fetched from the
        // control plane
        if !info.storage.persist {
            if let Some(manager) = manager.as_mut() {
                info!("wiping old checkpoints for {storage_id}");
                manager.wipe();
            }
        }
    }

    let tar_path = storage_path.join(LEDGER_STORAGE_FILE);

    // A reset height will not require untarring the ledger because it is
    // created from the genesis block
    if is_new_env && !height.reset() && tar_path.exists() {
        changed = true;

        // ensure the storage directory exists
        tokio::fs::create_dir_all(&ledger_dir)
            .await
            .map_err(|err| {
                error!("failed to create storage directory: {err}");
                ReconcileError::StorageSetupError("create ledger directory".to_string())
            })?;

        trace!("untarring ledger...");

        // use `tar` to decompress the storage to the untar dir
        let status = Command::new("tar")
            .current_dir(untar_base)
            .arg("xzf")
            .arg(&tar_path)
            .arg("-C") // the untar_dir must exist. this will extract the contents of the tar to the
            // directory
            .arg(untar_dir)
            .arg("--strip-components") // remove the parent "ledger" directory within the tar
            .arg("1")
            .kill_on_drop(true)
            .spawn()
            .map_err(|err| {
                error!("failed to spawn tar process: {err}");
                ReconcileError::StorageSetupError("spawn tar process".to_string())
            })?
            .wait()
            .await
            .map_err(|err| {
                error!("failed to await tar process: {err}");
                ReconcileError::StorageSetupError("await tar process".to_string())
            })?;

        if !status.success() {
            return Err(ReconcileError::StorageSetupError(format!(
                "tar failed: {status}"
            )));
        }
    }

    if matches!(height, HeightRequest::Top | HeightRequest::Absolute(0)) {
        return Ok(changed);
    }

    // retention policies are required for the rewind operations
    let Some(manager) = &manager.as_mut() else {
        return Err(ReconcileError::MissingRetentionPolicy);
    };

    // determine which checkpoint to use by the next available height/time
    let checkpoint = match height {
        HeightRequest::Absolute(block_height) => {
            checkpoint::find_by_height(manager, &info.storage.checkpoints, *block_height)
        }
        HeightRequest::Checkpoint(span) => {
            checkpoint::find_by_span(manager, &info.storage.checkpoints, *span)
        }
        _ => unreachable!("handled by previous match"),
    }
    .ok_or(ReconcileError::CheckpointAcquireError)?;

    // download checkpoint if necessary, and get the path
    let path = checkpoint
        .acquire(state, &storage_path, *storage_id, info.network)
        .await?;

    // apply the checkpoint to the ledger
    let mut command = Command::new(state.cli.path.join(SNARKOS_FILE));
    command
        .stdout(std::io::stdout())
        .stderr(std::io::stderr())
        .env("NETWORK", info.network.to_string())
        .arg("ledger")
        .arg("--ledger")
        .arg(&ledger_dir);

    if !info.storage.native_genesis {
        command
            .arg("--genesis")
            .arg(storage_path.join(SNARKOS_GENESIS_FILE));
    }

    command.arg("checkpoint").arg("apply").arg(path);

    let res = command
        .spawn()
        .map_err(|e| {
            error!("failed to spawn checkpoint apply process: {e}");
            ReconcileError::CheckpointApplyError("spawn checkpoint apply process".to_string())
        })?
        .wait()
        .await
        .map_err(|e| {
            error!("failed to await checkpoint apply process: {e}");
            ReconcileError::CheckpointApplyError("await checkpoint apply process".to_string())
        })?;

    if !res.success() {
        return Err(ReconcileError::CheckpointApplyError(format!(
            "checkpoint apply failed: {res}"
        )));
    }

    Ok(true)
}

async fn get_version_from_path(path: &PathBuf) -> Result<Option<u16>, ReconcileError> {
    if !path.exists() {
        return Ok(None);
    }

    let data = tokio::fs::read_to_string(path).await.map_err(|e| {
        error!("failed to read storage version: {e}");
        ReconcileError::StorageSetupError("failed to read storage version".to_string())
    })?;

    Ok(data.parse().ok())
}
