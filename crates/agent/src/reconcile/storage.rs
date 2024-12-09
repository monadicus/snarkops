use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use snops_checkpoint::CheckpointManager;
use snops_common::{
    api::AgentEnvInfo,
    binaries::{BinaryEntry, BinarySource},
    constant::{
        LEDGER_BASE_DIR, LEDGER_PERSIST_DIR, NODE_DATA_DIR, SNARKOS_FILE, SNARKOS_GENESIS_FILE,
        VERSION_FILE,
    },
    rpc::error::ReconcileError,
    state::{HeightRequest, InternedId, ReconcileCondition, ReconcileStatus, TransferId},
};
use tokio::{process::Command, sync::Mutex, task::AbortHandle};
use tracing::{error, info, trace};
use url::Url;

use super::{default_binary, get_genesis_route, DirectoryReconciler, FileReconciler, Reconcile};
use crate::state::GlobalState;

/// Download a specific binary file needed to run the node
pub struct BinaryReconciler<'a> {
    pub state: Arc<GlobalState>,
    pub env_info: Arc<AgentEnvInfo>,
    pub node_binary: Option<InternedId>,
    /// Metadata about an active binary transfer
    pub transfer: &'a mut Option<(TransferId, BinaryEntry)>,
    /// Time the binary was marked as OK
    pub ok_at: &'a mut Option<Instant>,
}

impl<'a> Reconcile<(), ReconcileError> for BinaryReconciler<'a> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError> {
        let BinaryReconciler {
            state,
            env_info,
            node_binary,
            transfer,
            ok_at,
        } = self;

        // Binary entry for the node
        let default_binary = default_binary(env_info);
        let target_binary = env_info
            .storage
            .binaries
            .get(&node_binary.unwrap_or_default())
            .unwrap_or(&default_binary);

        // Check if the binary has changed
        let binary_has_changed = transfer.as_ref().is_none_or(|(_, b)| b != target_binary);

        let dst = state.cli.path.join(SNARKOS_FILE);

        // The binary does not exist and is marked as OK...
        if ok_at.is_some() && (binary_has_changed || !dst.exists()) {
            **ok_at = None;
        }

        // If the binary has not changed and has not expired, we can skip the binary
        // reconciler
        if !binary_has_changed && ok_at.is_some() {
            return Ok(ReconcileStatus::default());
        }
        **ok_at = None;

        let src = match &target_binary.source {
            BinarySource::Url(url) => url.clone(),
            BinarySource::Path(path) => {
                let url = format!("{}{}", &state.endpoint, path.display());
                url.parse::<reqwest::Url>()
                    .map_err(|e| ReconcileError::UrlParseError(url, e.to_string()))?
            }
        };

        let mut file_rec = FileReconciler::new(Arc::clone(state), src, dst)
            .with_offline(target_binary.is_api_file() && !state.is_ws_online())
            .with_binary(target_binary)
            .with_tx_id(transfer.as_ref().map(|(tx, _)| *tx));
        let file_res = file_rec.reconcile().await?;

        **transfer = file_rec.tx_id.map(|tx_id| (tx_id, target_binary.clone()));

        // Transfer is pending or a failure occurred
        if file_res.is_requeue() {
            return Ok(file_res.emptied().add_scope("file/requeue"));
        }

        match file_res.inner {
            // If the binary is OK, update the context
            Some(true) => {
                **ok_at = Some(Instant::now());
                Ok(ReconcileStatus::default())
            }
            // If the binary is not OK, we will wait for the endpoint to come back
            // online...
            Some(false) => {
                trace!("binary is not OK, waiting for the endpoint to come back online...");
                Ok(ReconcileStatus::empty()
                    .add_condition(ReconcileCondition::PendingConnection)
                    .add_condition(ReconcileCondition::MissingFile {
                        path: SNARKOS_FILE.to_string(),
                    })
                    .add_scope("binary/offline")
                    .requeue_after(Duration::from_secs(5)))
            }
            None => unreachable!("file reconciler returns a result when not requeued"),
        }
    }
}

/// Download the genesis block needed to run the node
pub struct GenesisReconciler<'a> {
    pub state: Arc<GlobalState>,
    pub env_info: Arc<AgentEnvInfo>,
    /// Metadata about an active genesis transfer
    pub transfer: &'a mut Option<TransferId>,
    /// Time the genesis was marked as OK
    pub ok_at: &'a mut Option<Instant>,
}

impl<'a> Reconcile<(), ReconcileError> for GenesisReconciler<'a> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError> {
        let GenesisReconciler {
            state,
            env_info,
            transfer,
            ok_at,
        } = self;

        let storage_path = state
            .cli
            .storage_path(env_info.network, env_info.storage.id);

        let genesis_file = storage_path.join(SNARKOS_GENESIS_FILE);

        // If the genesis file doesn't exist, it's not okay...
        if !genesis_file.exists() && ok_at.is_some() {
            **ok_at = None;
        }

        // Genesis block file has been checked within 5 minutes
        let genesis_file_ok = ok_at
            .map(|ok| ok.elapsed().as_secs() < 300)
            .unwrap_or(false);

        if env_info.storage.native_genesis || genesis_file_ok {
            return Ok(ReconcileStatus::default());
        }
        **ok_at = None;

        let genesis_url = get_genesis_route(&state.endpoint, env_info.network, env_info.storage.id);
        let mut file_rec = FileReconciler::new(
            Arc::clone(&self.state),
            genesis_url.parse::<Url>().map_err(|e| {
                ReconcileError::UrlParseError(genesis_url.to_string(), e.to_string())
            })?,
            genesis_file,
        )
        .with_offline(!self.state.is_ws_online())
        .with_tx_id(**transfer);
        let file_res = file_rec.reconcile().await?;

        **transfer = file_rec.tx_id;

        if file_res.is_requeue() {
            return Ok(file_res.emptied().add_scope("file/requeue"));
        }

        match file_res.inner {
            // If the binary is OK, update the context
            Some(true) => {
                **ok_at = Some(Instant::now());
                Ok(ReconcileStatus::default())
            }
            // If the binary is not OK, we will wait for the endpoint to come back
            // online...
            Some(false) => {
                trace!("genesis is not OK, waiting for the endpoint to come back online...");
                Ok(ReconcileStatus::empty()
                    .add_condition(ReconcileCondition::PendingConnection)
                    .add_condition(ReconcileCondition::MissingFile {
                        path: SNARKOS_GENESIS_FILE.to_string(),
                    })
                    .add_scope("genesis/offline")
                    .requeue_after(Duration::from_secs(5)))
            }
            None => unreachable!("file reconciler returns a result when not requeued"),
        }
    }
}

pub type LedgerModifyResult = Result<bool, ReconcileError>;

pub struct LedgerReconciler<'a> {
    pub state: Arc<GlobalState>,
    pub env_info: Arc<AgentEnvInfo>,
    pub target_height: (usize, HeightRequest),
    pub last_height: &'a mut Option<(usize, HeightRequest)>,
    pub pending_height: &'a mut Option<(usize, HeightRequest)>,
    pub modify_handle: &'a mut Option<(AbortHandle, Arc<Mutex<Option<LedgerModifyResult>>>)>,
}

impl LedgerReconciler<'_> {
    pub fn untar_paths(&self) -> (PathBuf, &'static str) {
        if self.env_info.storage.persist {
            (
                self.state
                    .cli
                    .storage_path(self.env_info.network, self.env_info.storage.id),
                LEDGER_PERSIST_DIR,
            )
        } else {
            (self.state.cli.path.join(NODE_DATA_DIR), LEDGER_BASE_DIR)
        }
    }

    pub fn ledger_path(&self) -> PathBuf {
        let (path, dir) = self.untar_paths();
        path.join(dir)
    }

    /// Find the checkpoint to apply to the ledger
    /// Guaranteed error when target height is not the top, 0, or unlimited span
    pub fn find_checkpoint(&self) -> Result<PathBuf, ReconcileError> {
        let (untar_base, ledger_dir) = self.untar_paths();
        let ledger_path = untar_base.join(ledger_dir);

        // If there's a retention policy, load the checkpoint manager
        // this is so we can wipe all leftover checkpoints for non-persisted storage
        // after resets or new environments
        let manager = self
            .env_info
            .storage
            .retention_policy
            .clone()
            .map(|policy| {
                trace!("loading checkpoints from {untar_base:?}...");
                CheckpointManager::load(ledger_path.clone(), policy).map_err(|e| {
                    error!("failed to load checkpoints: {e}");
                    ReconcileError::CheckpointLoadError(e.to_string())
                })
            })
            .transpose()?
            .ok_or(ReconcileError::MissingRetentionPolicy(self.target_height.1))?;

        // Determine which checkpoint to use by the next available height/time
        match self.target_height.1 {
            HeightRequest::Absolute(height) => manager.nearest_with_height(height),
            HeightRequest::Checkpoint(span) => manager.nearest_with_span(span),
            // top cannot be a target height
            _ => None,
        }
        .map(|(_, path)| path)
        .ok_or(ReconcileError::NoAvailableCheckpoints(self.target_height.1))
        .cloned()
    }

    pub fn spawn_modify(
        &self,
        checkpoint: PathBuf,
    ) -> (AbortHandle, Arc<Mutex<Option<LedgerModifyResult>>>) {
        let result = Arc::new(Mutex::new(None));
        let result2 = Arc::clone(&result);

        let is_native_genesis = self.env_info.storage.native_genesis;
        let snarkos_path = self.state.cli.path.join(SNARKOS_FILE);
        let network = self.env_info.network;
        let storage_path = self
            .state
            .cli
            .storage_path(network, self.env_info.storage.id);
        let ledger_path = self.ledger_path();

        // apply the checkpoint to the ledger
        let mut command = Command::new(snarkos_path);
        command
            .stdout(std::io::stdout())
            .stderr(std::io::stderr())
            .env("NETWORK", network.to_string())
            .arg("ledger")
            .arg("--ledger")
            .arg(&ledger_path);

        if !is_native_genesis {
            command
                .arg("--genesis")
                .arg(storage_path.join(SNARKOS_GENESIS_FILE));
        }

        command.arg("checkpoint").arg("apply").arg(checkpoint);

        let handle = tokio::spawn(async move {
            let mut mutex = result.lock().await;

            let res = command
                .spawn()
                .map_err(|e| {
                    error!("failed to spawn checkpoint apply process: {e}");
                    mutex.replace(Err(ReconcileError::CheckpointApplyError(String::from(
                        "spawn checkpoint apply process",
                    ))));
                })?
                .wait()
                .await
                .map_err(|e| {
                    error!("failed to await checkpoint apply process: {e}");
                    mutex.replace(Err(ReconcileError::CheckpointApplyError(String::from(
                        "await checkpoint apply process",
                    ))));
                })?;

            mutex.replace(Ok(res.success()));

            Ok::<(), ()>(())
        })
        .abort_handle();

        (handle, result2)
    }
}

impl<'a> Reconcile<(), ReconcileError> for LedgerReconciler<'a> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError> {
        let env_info = self.env_info.clone();
        let target_height = self.target_height;

        let ledger_path = self.ledger_path();

        // Ledger reconcile behavior is different depending on whether the storage is
        // persistent.
        let is_persist = env_info.storage.persist;

        // Defaulting the initial height allows the reconciler to treat
        // a persisted env with non-top target heights as a request to delete
        // the ledger
        if self.last_height.is_none() {
            // The default last height is top
            *self.last_height = Some((0, HeightRequest::Top));

            //  delete ledger because no last_height indicates a fresh env
            if !is_persist {
                let _ = tokio::fs::remove_dir_all(&ledger_path).await;
            }
        }
        let last_height = self.last_height.unwrap();

        // TODO: only call this after unpacking the ledger
        // create the ledger path if it doesn't exist
        DirectoryReconciler(&ledger_path.join(".aleo/storage"))
            .reconcile()
            .await?;

        // If there is no pending height, check if there should be a pending height
        if self.pending_height.is_none() {
            // target height has been realized
            if last_height == target_height {
                return Ok(ReconcileStatus::default());
            }

            // If the target height is the top, we can skip the ledger reconciler
            if target_height.1.is_top() {
                *self.last_height = Some(target_height);
                if let Err(e) = self.state.db.set_last_height(Some(target_height)) {
                    error!("failed to save last height to db: {e}");
                }

                // ledger operation is complete
                return Ok(ReconcileStatus::default());
            }

            // If the target height is 0, we can delete the ledger
            if target_height.1.reset() {
                let _ = tokio::fs::remove_dir_all(&ledger_path).await;
                *self.last_height = Some(target_height);
                if let Err(e) = self.state.db.set_last_height(Some(target_height)) {
                    error!("failed to save last height to db: {e}");
                }

                // Ledger operation is complete... immediately requeue because the ledger was
                // wiped
                return Ok(ReconcileStatus::default()
                    .add_scope("ledger/wipe")
                    .requeue_after(Duration::ZERO));
            }

            // Target height is guaranteed to be different, not top, and not 0, which means
            // it's up to the retention policies

            // TODO: implement a heightrequest that downloads a remote ledger
            // TODO: ledger URL handling here instead of retention policy
            // TODO: ledger downloading would enter a new code path that downloads a new one

            // Find the checkpoint for the reconciler's target height
            let checkpoint = self.find_checkpoint()?;
            trace!("Applying checkpoint: {}", checkpoint.display());
            // Start a task to modify the ledger with the checkpoint
            *self.modify_handle = Some(self.spawn_modify(checkpoint));
            // Now that a task is running, set the pending height
            *self.pending_height = Some(target_height);
            trace!("Pending ledger modification to height {}", target_height.1);

            return Ok(ReconcileStatus::empty()
                .add_condition(ReconcileCondition::PendingProcess {
                    process: format!("ledger modification to height {}", target_height.1),
                })
                .requeue_after(Duration::from_secs(5)));
        }

        let Some(modify_handle) = self.modify_handle.as_ref() else {
            // This should be an unreachable condition, but may not be unreachable
            // when more complex ledger operations are implemented
            error!("modify handle missing for pending height");
            *self.pending_height = None;
            return Ok(ReconcileStatus::empty()
                .add_condition(ReconcileCondition::InterruptedModify {
                    reason: String::from("modify handle missing"),
                })
                .requeue_after(Duration::from_secs(1)));
        };

        // If the modify handle is locked, requeue until it's unlocked
        let Ok(Some(handle)) = modify_handle.1.try_lock().map(|r| r.clone()) else {
            trace!("Waiting for modify handle to unlock...");
            return Ok(ReconcileStatus::empty()
                .add_condition(ReconcileCondition::PendingProcess {
                    process: format!("ledger modification to height {}", target_height.1),
                })
                .requeue_after(Duration::from_secs(1)));
        };

        let pending = self.pending_height.unwrap();

        match handle {
            // If the ledger was modified successfully, update the last height
            Ok(true) => {
                info!(
                    "Ledger modification to height {} succeeded",
                    target_height.1
                );
                *self.last_height = Some(pending);
                if let Err(e) = self.state.db.set_last_height(Some(pending)) {
                    error!("failed to save last height to db: {e}");
                }
            }
            // A failure in the ledger modification process is handled at the
            // moment...
            Ok(false) => {
                error!("ledger modification to height {} failed", target_height.1);
                // TODO: handle this failure.. maybe even by deleting the ledger
            }
            // Bubble an actual error up to the caller
            Err(err) => {
                error!(
                    "ledger modification to height {} errored: {err}",
                    target_height.1
                );
                return Err(err.clone());
            }
        };

        // Modification is complete. The last height is change dhwen the modification
        // succeeds (above)
        *self.pending_height = None;
        *self.modify_handle = None;

        Ok(ReconcileStatus::default())
    }
}

pub struct StorageVersionReconciler<'a>(pub &'a Path, pub u16);

impl<'a> Reconcile<(), ReconcileError> for StorageVersionReconciler<'a> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError> {
        let StorageVersionReconciler(path, version) = self;

        let version_file = path.join(VERSION_FILE);

        let version_file_data = if !version_file.exists() {
            None
        } else {
            tokio::fs::read_to_string(&version_file)
                .await
                .map_err(|e| ReconcileError::FileReadError(version_file.clone(), e.to_string()))?
                .parse()
                .ok()
        };

        if path.exists() {
            // wipe old storage when the version changes
            if version_file_data != Some(*version) {
                info!("Removing storage directory for version mismatch: local {version_file_data:?} != remote {version:?}");
                let _ = tokio::fs::remove_dir_all(&path).await;
            } else {
                // return an empty status if the version is the same
                return Ok(ReconcileStatus::empty());
            };
        }

        DirectoryReconciler(path).reconcile().await?;

        if !version_file.exists() {
            tokio::fs::write(&version_file, version.to_string())
                .await
                .map_err(|e| {
                    error!("failed to write storage version: {e}");
                    ReconcileError::CreateDirectory(version_file.to_path_buf(), e.to_string())
                })?;
        }

        Ok(ReconcileStatus::default())
    }
}
