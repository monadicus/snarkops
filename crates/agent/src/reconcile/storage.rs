use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use snops_common::{
    api::EnvInfo,
    binaries::{BinaryEntry, BinarySource},
    constant::{SNARKOS_FILE, VERSION_FILE},
    rpc::error::ReconcileError2,
    state::{InternedId, TransferId},
};
use tracing::trace;

use super::{default_binary, FileReconciler, Reconcile, ReconcileCondition, ReconcileStatus};
use crate::state::GlobalState;

/// Download a specific binary file needed to run the node
pub struct BinaryReconciler<'a> {
    pub state: Arc<GlobalState>,
    pub env_info: Arc<EnvInfo>,
    pub node_binary: Option<InternedId>,
    /// Metadata about an active binary transfer
    pub binary_transfer: &'a mut Option<(TransferId, BinaryEntry)>,
    /// Time the binary was marked as OK
    pub binary_ok_at: &'a mut Option<Instant>,
}

impl<'a> Reconcile<(), ReconcileError2> for BinaryReconciler<'a> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError2> {
        let BinaryReconciler {
            state,
            env_info,
            node_binary,
            binary_transfer,
            binary_ok_at,
        } = self;

        // Binary entry for the node
        let default_binary = default_binary(env_info);
        let target_binary = env_info
            .storage
            .binaries
            .get(&node_binary.unwrap_or_default())
            .unwrap_or(&default_binary);

        // Check if the binary has changed
        let binary_has_changed = binary_transfer
            .as_ref()
            .map(|(_, b)| b != target_binary)
            .unwrap_or(true);
        let binary_is_ok = binary_ok_at
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

        let is_api_offline = state.client.read().await.is_none();

        let file_res = FileReconciler::new(Arc::clone(state), src, dst)
            .with_offline(target_binary.is_api_file() && is_api_offline)
            .with_binary(target_binary)
            .with_tx_id(binary_transfer.as_ref().map(|(tx, _)| *tx))
            .reconcile()
            .await?;

        // transfer is pending or a failure occurred
        if file_res.is_requeue() {
            return Ok(file_res.emptied().add_scope("file_reconcile/requeue"));
        }

        match file_res.inner {
            // If the binary is OK, update the context
            Some(true) => {
                **binary_ok_at = Some(Instant::now());
                Ok(ReconcileStatus::default())
            }
            // If the binary is not OK, we will wait for the endpoint to come back
            // online...
            Some(false) => {
                trace!("binary is not OK, waiting for the endpoint to come back online...");
                Ok(ReconcileStatus::empty()
                    .add_condition(ReconcileCondition::PendingConnection)
                    .add_scope("agent_state/binary/offline")
                    .requeue_after(Duration::from_secs(5)))
            }
            None => unreachable!("file reconciler returns a result when not requeued"),
        }
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
