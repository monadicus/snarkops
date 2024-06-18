use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use checkpoint::{CheckpointHeader, CheckpointManager, RetentionSpan};
use snops_common::{
    api::{CheckpointMeta, EnvInfo},
    constant::{
        LEDGER_BASE_DIR, LEDGER_PERSIST_DIR, LEDGER_STORAGE_FILE, SNARKOS_FILE,
        SNARKOS_GENESIS_FILE, VERSION_FILE,
    },
    rpc::error::ReconcileError,
    state::{EnvId, HeightRequest, NetworkId, StorageId},
};
use tokio::process::Command;
use tracing::{debug, error, info, trace};

use crate::{api, state::GlobalState};

/// Ensure all required files are present in the storage directory
pub async fn check_files(
    state: &GlobalState,
    env_id: EnvId,
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

    // TODO: store binary based on binary id
    // download the snarkOS binary
    api::check_binary(
        env_id,
        &state.endpoint,
        &base_path.join(SNARKOS_FILE),
        state.transfer_tx(),
    ) // TODO: http(s)?
    .await
    .expect("failed to acquire snarkOS binary");

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
    let genesis_url = format!(
        "{}/content/storage/{network}/{storage_id}/{SNARKOS_GENESIS_FILE}",
        &state.endpoint
    );
    let ledger_path = storage_path.join(LEDGER_STORAGE_FILE);
    let ledger_url = format!(
        "{}/content/storage/{network}/{storage_id}/{LEDGER_STORAGE_FILE}",
        &state.endpoint
    );

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
            find_checkpoint_by_height(manager, &info.storage.checkpoints, *block_height)
        }
        HeightRequest::Checkpoint(span) => {
            find_checkpoint_by_span(manager, &info.storage.checkpoints, *span)
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
            .arg(&storage_path.join(SNARKOS_GENESIS_FILE));
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

enum CheckpointSource<'a> {
    Manager(&'a CheckpointHeader, &'a PathBuf),
    Meta(&'a CheckpointMeta),
}

impl<'a> CheckpointSource<'a> {
    async fn acquire(
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

fn find_checkpoint_by_height<'a>(
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

fn find_checkpoint_by_span<'a>(
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
