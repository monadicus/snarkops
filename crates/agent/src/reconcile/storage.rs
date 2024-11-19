use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use snops_common::{
    api::EnvInfo,
    binaries::{BinaryEntry, BinarySource},
    constant::{
        LEDGER_BASE_DIR, LEDGER_PERSIST_DIR, SNARKOS_FILE, SNARKOS_GENESIS_FILE, VERSION_FILE,
    },
    rpc::error::ReconcileError2,
    state::{HeightRequest, InternedId, TransferId},
};
use tokio::{sync::Mutex, task::AbortHandle};
use tracing::trace;
use url::Url;

use super::{
    default_binary, get_genesis_route, DirectoryReconciler, FileReconciler, Reconcile,
    ReconcileCondition, ReconcileStatus,
};
use crate::state::GlobalState;

/// Download a specific binary file needed to run the node
pub struct BinaryReconciler<'a> {
    pub state: Arc<GlobalState>,
    pub env_info: Arc<EnvInfo>,
    pub node_binary: Option<InternedId>,
    /// Metadata about an active binary transfer
    pub transfer: &'a mut Option<(TransferId, BinaryEntry)>,
    /// Time the binary was marked as OK
    pub ok_at: &'a mut Option<Instant>,
}

impl<'a> Reconcile<(), ReconcileError2> for BinaryReconciler<'a> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError2> {
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
        let binary_has_changed = transfer
            .as_ref()
            .map(|(_, b)| b != target_binary)
            .unwrap_or(true);
        let binary_is_ok = ok_at
            .map(|ok| ok.elapsed().as_secs() < 300) // check if the binary has been OK for 5 minutes
            .unwrap_or(false);

        // If the binary has not changed and has not expired, we can skip the binary
        // reconciler
        if !binary_has_changed && binary_is_ok {
            return Ok(ReconcileStatus::default());
        }

        let src = match &target_binary.source {
            BinarySource::Url(url) => url.clone(),
            BinarySource::Path(path) => {
                let url = format!("{}{}", &state.endpoint, path.display());
                url.parse::<reqwest::Url>()
                    .map_err(|e| ReconcileError2::UrlParseError(url, e.to_string()))?
            }
        };
        let dst = state.cli.path.join(SNARKOS_FILE);

        let mut file_rec = FileReconciler::new(Arc::clone(state), src, dst)
            .with_offline(target_binary.is_api_file() && !state.is_ws_online())
            .with_binary(target_binary)
            .with_tx_id(transfer.as_ref().map(|(tx, _)| *tx));
        let file_res = file_rec.reconcile().await?;
        if let Some(tx_id) = file_rec.tx_id {
            **transfer = Some((tx_id, target_binary.clone()));
        }

        // transfer is pending or a failure occurred
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
                    .add_condition(ReconcileCondition::MissingFile(SNARKOS_FILE.to_string()))
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
    pub env_info: Arc<EnvInfo>,
    /// Metadata about an active genesis transfer
    pub transfer: &'a mut Option<TransferId>,
    /// Time the genesis was marked as OK
    pub ok_at: &'a mut Option<Instant>,
}

impl<'a> Reconcile<(), ReconcileError2> for GenesisReconciler<'a> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError2> {
        let GenesisReconciler {
            state,
            env_info,
            transfer,
            ok_at,
        } = self;

        let storage_path = state
            .cli
            .storage_path(env_info.network, env_info.storage.id);

        // Genesis block file has been checked within 5 minutes
        let genesis_file_ok = ok_at
            .map(|ok| ok.elapsed().as_secs() < 300)
            .unwrap_or(false);

        if env_info.storage.native_genesis || !genesis_file_ok {
            return Ok(ReconcileStatus::default());
        }

        let genesis_url = get_genesis_route(&state.endpoint, env_info.network, env_info.storage.id);
        let mut file_rec = FileReconciler::new(
            Arc::clone(&self.state),
            genesis_url.parse::<Url>().map_err(|e| {
                ReconcileError2::UrlParseError(genesis_url.to_string(), e.to_string())
            })?,
            storage_path.join(SNARKOS_GENESIS_FILE),
        )
        .with_offline(!self.state.is_ws_online())
        .with_tx_id(**transfer);
        let file_res = file_rec.reconcile().await?;

        if let Some(tx_id) = file_rec.tx_id {
            **transfer = Some(tx_id);
        }

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
                    .add_condition(ReconcileCondition::MissingFile(
                        SNARKOS_GENESIS_FILE.to_string(),
                    ))
                    .add_scope("genesis/offline")
                    .requeue_after(Duration::from_secs(5)))
            }
            None => unreachable!("file reconciler returns a result when not requeued"),
        }
    }
}

pub type LedgerModifyResult = Result<bool, ReconcileError2>;

pub struct LedgerReconciler<'a> {
    pub state: Arc<GlobalState>,
    pub env_info: Arc<EnvInfo>,
    pub target_height: HeightRequest,
    pub last_height: &'a mut Option<HeightRequest>,
    pub pending_height: &'a mut Option<HeightRequest>,
    pub ok_at: &'a mut Option<Instant>,
    pub transfer: &'a mut Option<TransferId>,
    pub modify_handle: &'a mut Option<(AbortHandle, Arc<Mutex<Option<LedgerModifyResult>>>)>,
    pub unpack_handle: &'a mut Option<(AbortHandle, Arc<Mutex<Option<LedgerModifyResult>>>)>,
}

impl<'a> Reconcile<(), ReconcileError2> for LedgerReconciler<'a> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError2> {
        let LedgerReconciler {
            state,
            env_info,
            ok_at,
            transfer,
            modify_handle,
            unpack_handle,
            target_height,
            last_height,
            pending_height,
        } = self;

        let network = env_info.network;
        let storage_id = env_info.storage.id;

        let (untar_base, untar_dir) = if env_info.storage.persist {
            (
                state.cli.storage_path(network, storage_id),
                LEDGER_PERSIST_DIR,
            )
        } else {
            (state.cli.path.clone(), LEDGER_BASE_DIR)
        };

        let ledger_path = untar_base.join(untar_dir);

        DirectoryReconciler(&ledger_path.join(".aleo"))
            .reconcile()
            .await?;

        // If the ledger is OK and the target height is the top, we can skip the ledger
        // reconciler
        if env_info.storage.persist && target_height.is_top() && ledger_path.exists() {
            return Ok(ReconcileStatus::default());
        }

        // TODO: if pending_height - check unpack/modify handles

        let is_new_env = last_height.is_none();

        Ok(ReconcileStatus::empty())
    }
}

pub struct StorageVersionReconciler<'a>(pub &'a Path, pub u16);

impl<'a> Reconcile<(), ReconcileError2> for StorageVersionReconciler<'a> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError2> {
        let StorageVersionReconciler(path, version) = self;

        let version_file = path.join(VERSION_FILE);

        let version_file_data = if !version_file.exists() {
            None
        } else {
            tokio::fs::read_to_string(&version_file)
                .await
                .map_err(|e| ReconcileError2::FileReadError(version_file.clone(), e.to_string()))?
                .parse()
                .ok()
        };

        // wipe old storage when the version changes
        Ok(if version_file_data != Some(*version) && path.exists() {
            let _ = tokio::fs::remove_dir_all(&path).await;
            ReconcileStatus::default()
        } else {
            // return an empty status if the version is the same
            ReconcileStatus::empty()
        })
    }
}
