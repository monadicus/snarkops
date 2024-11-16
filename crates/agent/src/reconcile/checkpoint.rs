use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use snops_checkpoint::{CheckpointHeader, CheckpointManager, RetentionSpan};
use snops_common::{
    api::CheckpointMeta,
    rpc::error::ReconcileError,
    state::{NetworkId, StorageId},
};
use tracing::{error, info};

use crate::{api, state::GlobalState};

pub enum CheckpointSource<'a> {
    Manager(&'a CheckpointHeader, &'a PathBuf),
    Meta(&'a CheckpointMeta),
}

impl<'a> CheckpointSource<'a> {
    pub async fn acquire(
        self,
        state: &GlobalState,
        storage_path: &Path,
        storage_id: StorageId,
        network: NetworkId,
    ) -> Result<PathBuf, ReconcileError> {
        Ok(match self {
            CheckpointSource::Meta(meta) => {
                info!(
                    "using checkpoint from control plane with height {} and time {}",
                    meta.height, meta.timestamp
                );
                let checkpoint_url = format!(
                    "{}/content/storage/{network}/{storage_id}/{}",
                    &state.endpoint, meta.filename
                );
                let path = storage_path.join(&meta.filename);
                info!("downloading {} from {checkpoint_url}...", meta.filename);

                api::check_file(checkpoint_url, &path, state.transfer_tx())
                    .await
                    .map_err(|e| {
                        error!(
                            "failed to download {} from the control plane: {e}",
                            meta.filename
                        );
                        ReconcileError::StorageAcquireError(meta.filename.clone())
                    })?;

                path
            }
            CheckpointSource::Manager(header, path) => {
                info!(
                    "using checkpoint from manager with height {} and time {}",
                    header.block_height,
                    header.time()
                );
                path.clone()
            }
        })
    }
}

pub fn find_by_height<'a>(
    manager: &'a CheckpointManager,
    checkpoints: &'a [CheckpointMeta],
    height: u32,
) -> Option<CheckpointSource<'a>> {
    let sorted: BTreeMap<_, _> = manager
        .checkpoints()
        .map(|(c, p)| (c.block_height, CheckpointSource::Manager(c, p)))
        .chain(
            checkpoints
                .iter()
                .map(|c| (c.height, CheckpointSource::Meta(c))),
        )
        .collect();

    sorted
        .into_iter()
        .rev()
        .find_map(|(h, c)| if h <= height { Some(c) } else { None })
}

pub fn find_by_span<'a>(
    manager: &'a CheckpointManager,
    checkpoints: &'a [CheckpointMeta],
    span: RetentionSpan,
) -> Option<CheckpointSource<'a>> {
    let timestamp = span.as_timestamp()?;

    let sorted: BTreeMap<_, _> = manager
        .checkpoints()
        .map(|(c, p)| (c.timestamp, CheckpointSource::Manager(c, p)))
        .chain(
            checkpoints
                .iter()
                .map(|c| (c.timestamp, CheckpointSource::Meta(c))),
        )
        .collect();

    sorted
        .into_iter()
        .rev()
        .find_map(|(t, c)| if t <= timestamp { Some(c) } else { None })
}
