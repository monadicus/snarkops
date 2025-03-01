use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use chrono::{TimeDelta, Utc};
use snops_common::{
    api::AgentEnvInfo,
    binaries::{BinaryEntry, BinarySource},
    constant::SNARKOS_GENESIS_FILE,
    rpc::error::ReconcileError,
    state::{
        NetworkId, ReconcileCondition, ReconcileStatus, StorageId, TransferId, TransferStatusUpdate,
    },
};
use tracing::{error, trace, warn};
use url::Url;

use super::Reconcile;
use crate::{
    api::{download_file, get_file_issues},
    state::GlobalState,
    transfers,
};

pub fn default_binary(info: &AgentEnvInfo) -> BinaryEntry {
    BinaryEntry {
        source: BinarySource::Path(PathBuf::from(format!(
            "/content/storage/{}/{}/binaries/default",
            info.network, info.storage.id
        ))),
        sha256: None,
        size: None,
    }
}

pub fn get_genesis_route(endpoint: &str, network: NetworkId, storage_id: StorageId) -> String {
    format!("{endpoint}/content/storage/{network}/{storage_id}/{SNARKOS_GENESIS_FILE}")
}

/// This reconciler creates a directory if it does not exist
pub struct DirectoryReconciler<'a>(pub &'a Path);
impl Reconcile<(), ReconcileError> for DirectoryReconciler<'_> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError> {
        std::fs::create_dir_all(self.0)
            .map(ReconcileStatus::with)
            .map_err(|e| ReconcileError::CreateDirectory(self.0.to_path_buf(), e.to_string()))
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

    pub fn check_and_set_mode(&self) -> Result<(), ReconcileError> {
        // ensure the file has the correct permissions
        let Some(check_perms) = self.permissions else {
            return Ok(());
        };

        let perms = self
            .dst
            .metadata()
            .map_err(|e| ReconcileError::FileStatError(self.dst.clone(), e.to_string()))?
            .permissions();

        if perms.mode() != check_perms {
            std::fs::set_permissions(&self.dst, std::fs::Permissions::from_mode(check_perms))
                .map_err(|e| {
                    ReconcileError::FilePermissionError(self.dst.clone(), e.to_string())
                })?;
        }

        Ok(())
    }
}

impl Reconcile<bool, ReconcileError> for FileReconciler {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<bool>, ReconcileError> {
        let client = reqwest::Client::new();

        // Create a transfer id if one is not provided
        if self.tx_id.is_none() {
            self.tx_id = Some(transfers::next_id());
        }

        let tx_id = self.tx_id.unwrap();

        // transfer is pending
        let is_complete = match self.state.transfers.entry(tx_id) {
            dashmap::Entry::Occupied(occupied_entry) => {
                let entry = occupied_entry.get();

                if entry.is_pending() {
                    return Ok(ReconcileStatus::empty()
                        .add_condition(ReconcileCondition::PendingTransfer {
                            source: self.src.to_string(),
                            id: tx_id,
                        })
                        .requeue_after(Duration::from_secs(1)));
                }

                if entry.is_interrupted() {
                    // if the failure is within the last 60 seconds, requeue
                    if Utc::now().signed_duration_since(entry.updated_at).abs()
                        < TimeDelta::seconds(60)
                    {
                        return Ok(ReconcileStatus::empty()
                            .add_condition(ReconcileCondition::InterruptedTransfer {
                                source: self.src.to_string(),
                                id: tx_id,
                                reason: entry.interruption.clone(),
                            })
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
                true
            }
            dashmap::Entry::Vacant(_) => false,
        };

        let file_problems = get_file_issues(
            &client,
            self.src.as_str(),
            self.dst.as_path(),
            self.check_size,
            self.check_sha256.as_deref(),
            self.offline,
        )
        .await?;

        // There is an issue with the file being complete and not existing
        if is_complete && !self.dst.exists() {
            // Clear the download
            self.tx_id = None;
            warn!(
                "File is complete but does not exist: {} (Problem: {file_problems:?})",
                self.dst.display()
            );

            return Ok(ReconcileStatus::empty()
                .add_condition(ReconcileCondition::MissingFile {
                    path: self.dst.display().to_string(),
                })
                .requeue_after(Duration::from_secs(1)));
        }

        if is_complete && file_problems.is_some() {
            warn!(
                "Complete file has {file_problems:?} problems: {}",
                self.dst.display()
            );

            // if the file is complete, but there are issues, requeue
            if self.dst.exists() {
                // delete the file
                tokio::fs::remove_file(&self.dst).await.map_err(|e| {
                    ReconcileError::DeleteFileError(self.dst.clone(), e.to_string())
                })?;
            }

            // Clear the download
            self.tx_id = None;

            return Ok(ReconcileStatus::empty()
                .add_condition(ReconcileCondition::MissingFile {
                    path: self.dst.display().to_string(),
                })
                .requeue_after(Duration::from_secs(1)));
        }

        // Everything is good. Ensure file permissions
        if file_problems.is_none() {
            self.check_and_set_mode()?;
            trace!("File reconcile complete: {}", self.dst.display());
            return Ok(ReconcileStatus::with(true));
        }

        // file does not exist and cannot be downloaded right now
        if !self.dst.exists() && self.offline {
            return Ok(
                ReconcileStatus::with(false).add_condition(ReconcileCondition::PendingConnection)
            );
        }

        let src = self.src.clone();
        let dst = self.dst.clone();
        let transfer_tx = self.state.transfer_tx.clone();

        // download the file
        let handle = tokio::spawn(async move {
            download_file(tx_id, &client, src, &dst, transfer_tx)
                .await
                // Dropping the File from download_file should close the handle
                .map(|res| res.is_some())
        })
        .abort_handle();

        // update the transfer with the handle (so it can be canceled if necessary)
        if let Err(e) = self
            .state
            .transfer_tx
            .send((tx_id, TransferStatusUpdate::Handle(handle)))
        {
            error!("failed to send transfer handle: {e}");
        }

        trace!(
            "Started download of {} to {} via tx_id {tx_id}",
            self.src,
            self.dst.display()
        );

        // transfer is pending - requeue after 1 second with the pending condition
        Ok(ReconcileStatus::empty()
            .add_condition(ReconcileCondition::PendingTransfer {
                source: self.src.to_string(),
                id: tx_id,
            })
            .requeue_after(Duration::from_secs(1)))
    }
}
