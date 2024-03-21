use std::{ops::Deref, process::Stdio, sync::Arc};

use futures::StreamExt;
use snot_common::{
    rpc::{
        agent::{AgentService, AgentServiceRequest, AgentServiceResponse, ReconcileError},
        control::{ControlServiceRequest, ControlServiceResponse},
        MuxMessage,
    },
    state::AgentState,
};
use tarpc::{context, ClientMessage, Response};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
};
use tracing::{debug, info, warn, Level};

use crate::state::AppState;

/// The JWT file name.
pub const JWT_FILE: &str = "jwt";
/// The snarkOS binary file name.
pub const SNARKOS_FILE: &str = "snarkos";
/// The snarkOS log file name.
pub const SNARKOS_LOG_FILE: &str = "snarkos.log";
/// The genesis block file name.
pub const SNARKOS_GENESIS_FILE: &str = "genesis.block";
/// The base genesis block file name.
pub const SNARKOS_GENESIS_BASE_FILE: &str = "genesis.block.base";
/// The ledger directory name.
pub const SNARKOS_LEDGER_DIR: &str = "ledger";
/// The base ledger directory name.
pub const SNARKOS_LEDGER_BASE_DIR: &str = "ledger.base";
/// Temporary storage archive file name.
pub const TEMP_STORAGE_FILE: &str = "storage.tar.gz";

/// A multiplexed message, incoming on the websocket.
pub type MuxedMessageIncoming =
    MuxMessage<Response<ControlServiceResponse>, ClientMessage<AgentServiceRequest>>;

/// A multiplexed message, outgoing on the websocket.
pub type MuxedMessageOutgoing =
    MuxMessage<ClientMessage<ControlServiceRequest>, Response<AgentServiceResponse>>;

// TODO: include agent state (process, JWT, etc.)
#[derive(Clone)]
pub struct AgentRpcServer {
    pub state: AppState,
}

impl AgentService for AgentRpcServer {
    async fn keep_jwt(self, _: context::Context, token: String) {
        debug!("control plane delegated new JWT");

        // cache the JWT in the state JWT mutex
        self.state
            .jwt
            .lock()
            .expect("failed to acquire JWT lock")
            .replace(token.to_owned());

        // TODO: write the JWT to a file somewhere else
        tokio::fs::write(self.state.cli.path.join(JWT_FILE), token)
            .await
            .expect("failed to write jwt file");
    }

    async fn reconcile(
        self,
        _: context::Context,
        target: AgentState,
    ) -> Result<(), ReconcileError> {
        if matches!(target, AgentState::Cannon(_, _)) {
            unimplemented!("tx cannons are unimplemented");
        }

        // acquire the handle lock
        let mut handle_container = self.state.reconcilation_handle.lock().await;

        // abort if we are already reconciling
        if let Some(handle) = handle_container.take() {
            handle.abort();
        }

        // perform the reconcilation
        let state = Arc::clone(&self.state);
        let handle = tokio::spawn(async move {
            // previous state cleanup
            let old_state = {
                let agent_state_lock = state.agent_state.read().await;
                match agent_state_lock.deref() {
                    // kill existing child if running
                    AgentState::Node(_, node) if node.online => {
                        if let Some(mut child) = state.child.write().await.take() {
                            child.kill().await.expect("failed to kill child process");
                        }
                    }

                    _ => (),
                }

                agent_state_lock.deref().clone()
            };

            // download new storage if storage_id changed
            'storage: {
                match (&old_state, &target) {
                    (AgentState::Node(old, _), AgentState::Node(new, _)) if old == new => {
                        // same storage_id
                        // TODO: check if we need to update the ledger height
                        break 'storage;
                    }

                    _ => (),
                }

                // clean up old storage
                let base_path = &state.cli.path;
                let filenames = &[
                    base_path.join(SNARKOS_GENESIS_FILE),
                    base_path.join(SNARKOS_GENESIS_BASE_FILE),
                ];
                let directories = &[
                    base_path.join(SNARKOS_LEDGER_DIR),
                    base_path.join(SNARKOS_LEDGER_BASE_DIR),
                ];

                for filename in filenames {
                    let _ = tokio::fs::remove_file(filename).await;
                }
                for dir in directories {
                    let _ = tokio::fs::remove_dir_all(dir).await;
                }

                // download and decompress the storage
                // skip if we don't need storage
                let AgentState::Node(storage_id, _) = &target else {
                    break 'storage;
                };

                // open a file for writing the archive
                let mut file = tokio::fs::File::create(base_path.join(TEMP_STORAGE_FILE))
                    .await
                    .map_err(|_| ReconcileError::StorageAcquireError)?;

                // stream the archive containing the storage
                let mut stream = reqwest::get(format!(
                    "http://{}/content/storage/{storage_id}.tar.gz",
                    &state.endpoint
                ))
                .await
                .map_err(|_| ReconcileError::StorageAcquireError)?
                .bytes_stream();

                // write the streamed archive to the file
                while let Some(chunk) = stream.next().await {
                    file.write_all(&chunk.map_err(|_| ReconcileError::StorageAcquireError)?)
                        .await
                        .map_err(|_| ReconcileError::StorageAcquireError)?;
                }

                let _ = (file, stream);

                // use `tar` to decompress the storage
                let mut tar_child = Command::new("tar")
                    .current_dir(&base_path)
                    .arg("-xzf")
                    .arg(TEMP_STORAGE_FILE)
                    .kill_on_drop(true)
                    .spawn()
                    .map_err(|_| ReconcileError::StorageAcquireError)?;

                let status = tar_child
                    .wait()
                    .await
                    .map_err(|_| ReconcileError::StorageAcquireError)?;

                // unconditionally remove the tar regardless of success
                let _ = tokio::fs::remove_file(base_path.join(TEMP_STORAGE_FILE)).await;

                // return an error if the storage acquisition failed
                if !status.success() {
                    return Err(ReconcileError::StorageAcquireError);
                }
            }

            // reconcile towards new state
            match target {
                // do nothing on inventory state
                AgentState::Inventory => (),

                // start snarkOS node when node
                AgentState::Node(_, node) => {
                    let mut child_lock = state.child.write().await;
                    let mut command = Command::new(state.cli.path.join(SNARKOS_FILE));

                    // TODO: more args
                    command
                        .stdout(Stdio::piped())
                        .arg("run")
                        .arg("--type")
                        .arg(node.ty.flag())
                        .arg("--log")
                        .arg(state.cli.path.join(SNARKOS_LOG_FILE))
                        .arg("--genesis")
                        .arg(state.cli.path.join(SNARKOS_GENESIS_FILE))
                        .arg("--ledger")
                        .arg(state.cli.path.join(SNARKOS_LEDGER_DIR));

                    if !node.peers.is_empty() {
                        // TODO: add peers

                        // TODO: local caching of agent IDs, map agent ID to
                        // IP/port
                    }

                    // TODO: same for validators

                    let mut child = command.spawn().expect("failed to start child");

                    // start a new task to log stdout
                    let stdout = child.stdout.take().unwrap();
                    tokio::spawn(async move {
                        let child_span = tracing::span!(Level::INFO, "child process stdout");
                        let _enter = child_span.enter();

                        let mut reader = BufReader::new(stdout).lines();
                        while let Ok(Some(line)) = reader.next_line().await {
                            info!(line);
                        }
                    });

                    *child_lock = Some(child);
                }

                // TODO
                AgentState::Cannon(_, _) => unimplemented!(),
            }

            Ok(())
        });

        // update the mutex with our new handle and drop the lock
        *handle_container = Some(handle.abort_handle());
        drop(handle_container);

        // await reconcilation completion
        let res = match handle.await {
            Err(e) if e.is_cancelled() => {
                warn!("reconcilation was aborted by a newer reconcilation request");

                // early return (don't clean up the handle lock)
                return Err(ReconcileError::Aborted);
            }

            Ok(inner) => inner,
            Err(e) => {
                warn!("reconcilation task panicked: {e}");
                Err(ReconcileError::Unknown)
            }
        };

        // clean up the abort handle
        // we can't be here if we were cancelled (see early return above)
        self.state.reconcilation_handle.lock().await.take();

        res
    }
}
