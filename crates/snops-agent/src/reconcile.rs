use snops_common::{
    api::StorageInfo,
    constant::{
        LEDGER_BASE_DIR, LEDGER_PERSIST_DIR, LEDGER_STORAGE_FILE, SNARKOS_FILE,
        SNARKOS_GENESIS_FILE,
    },
    rpc::error::ReconcileError,
    state::{EnvId, HeightRequest},
};
use tokio::process::Command;
use tracing::{error, info, trace};

use crate::{api, state::GlobalState};

/// Ensure all required files are present in the storage directory
pub async fn check_files(
    state: &GlobalState,
    env_id: EnvId,
    info: &StorageInfo,
    height: &HeightRequest,
) -> Result<(), ReconcileError> {
    let base_path = &state.cli.path;
    let storage_id = &info.id;
    let storage_path = base_path.join("storage").join(storage_id);

    // create the directory containing the storage files
    tokio::fs::create_dir_all(&storage_path)
        .await
        .map_err(|_| ReconcileError::StorageAcquireError)?;

    // TODO: store binary based on binary id
    // download the snarkOS binary
    api::check_binary(
        env_id,
        &format!("http://{}", &state.endpoint),
        &base_path.join(SNARKOS_FILE),
    ) // TODO: http(s)?
    .await
    .expect("failed to acquire snarkOS binary");

    let genesis_url = format!(
        "http://{}/content/storage/{storage_id}/{SNARKOS_GENESIS_FILE}",
        &state.endpoint
    );

    // download the genesis block
    api::check_file(genesis_url, &storage_path.join(SNARKOS_GENESIS_FILE))
        .await
        .map_err(|_| ReconcileError::StorageAcquireError)?;

    // don't download
    if height.reset() {
        info!("skipping ledger check due to 0 height request");
        return Ok(());
    }

    let ledger_url = format!(
        "http://{}/content/storage/{storage_id}/{LEDGER_STORAGE_FILE}",
        &state.endpoint
    );

    // download the ledger file
    api::check_file(ledger_url, &storage_path.join(LEDGER_STORAGE_FILE))
        .await
        .map_err(|_| ReconcileError::StorageAcquireError)?;

    Ok(())
}

/// Untar the ledger file into the storage directory
pub async fn load_ledger(
    state: &GlobalState,
    info: &StorageInfo,
    height: &HeightRequest,
    new_env: bool,
) -> Result<bool, ReconcileError> {
    let base_path = &state.cli.path;
    let storage_id = &info.id;
    let storage_path = base_path.join("storage").join(storage_id);

    // use a persisted directory for the untar when configured
    let (untar_base, untar_dir) = if info.persist {
        info!("using persisted ledger for {storage_id}");
        (&storage_path, LEDGER_PERSIST_DIR)
    } else {
        info!("using fresh ledger for {storage_id}");
        (base_path, LEDGER_BASE_DIR)
    };

    let ledger_dir = untar_base.join(untar_dir);

    // skip the top request if the persisted ledger already exists
    // this will prevent the ledger from getting wiped in the next step
    if info.persist && height.is_top() && ledger_dir.exists() {
        info!("persisted ledger already exists for {storage_id}");
        return Ok(false);
    }

    // reload the storage if the height is reset or a new environment is created
    if height.reset() || new_env {
        // clean up old storage
        if ledger_dir.exists() {
            if let Err(err) = tokio::fs::remove_dir_all(&ledger_dir).await {
                error!("failed to remove old ledger: {err}");
            }
        }

        // ensure the storage directory exists
        tokio::fs::create_dir_all(&ledger_dir)
            .await
            .map_err(|err| {
                error!("failed to create storage directory: {err}");
                ReconcileError::StorageAcquireError
            })?;

        trace!("untarring ledger...");

        // use `tar` to decompress the storage to the untar dir
        let status = Command::new("tar")
            .current_dir(untar_base)
            .arg("xzf")
            .arg(&storage_path.join(LEDGER_STORAGE_FILE))
            .arg("-C") // the untar_dir must exist. this will extract the contents of the tar to the
            // directory
            .arg(untar_dir)
            .kill_on_drop(true)
            .spawn()
            .map_err(|err| {
                error!("failed to spawn tar process: {err}");
                ReconcileError::StorageAcquireError
            })?
            .wait()
            .await
            .map_err(|err| {
                error!("failed to await tar process: {err}");
                ReconcileError::StorageAcquireError
            })?;

        if !status.success() {
            return Err(ReconcileError::StorageAcquireError);
        }
    }

    Ok(true)
}
