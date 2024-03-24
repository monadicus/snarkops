use std::{collections::HashSet, net::IpAddr, ops::Deref, process::Stdio, sync::Arc};

use snot_common::{
    rpc::{
        agent::{
            AgentError, AgentService, AgentServiceRequest, AgentServiceResponse, ReconcileError,
        },
        control::{ControlServiceRequest, ControlServiceResponse},
        MuxMessage,
    },
    state::{AgentId, AgentPeer, AgentState, PortConfig},
};
use tarpc::{context, ClientMessage, Response};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use tracing::{debug, error, info, warn, Level};

use crate::{api, state::AppState};

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
pub const LEDGER_STORAGE_FILE: &str = "ledger.tar.gz";

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
        info!("beginning reconcilation...");

        // acquire the handle lock
        let mut handle_container = self.state.reconcilation_handle.lock().await;

        // abort if we are already reconciling
        if let Some(handle) = handle_container.take() {
            info!("aborting previous reconcilation task...");
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
                        info!("cleaning up snarkos process...");
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

                // TODO: download storage to a cache directory

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

                let genesis_url = format!(
                    "http://{}/api/v1/storage/{storage_id}/genesis",
                    &state.endpoint
                );

                let ledger_url = format!(
                    "http://{}/api/v1/storage/{storage_id}/ledger",
                    &state.endpoint
                );

                // download the genesis block
                api::download_file(genesis_url, base_path.join(SNARKOS_GENESIS_FILE))
                    .await
                    .map_err(|_| ReconcileError::StorageAcquireError)?;

                // download the ledger
                let mut fail = false;

                if let Ok(Some(())) =
                    api::download_file(ledger_url, base_path.join(LEDGER_STORAGE_FILE))
                        .await
                        .map_err(|_| ReconcileError::StorageAcquireError)
                {
                    // TODO: remove existing ledger probably

                    // use `tar` to decompress the storage
                    let mut tar_child = Command::new("tar")
                        .current_dir(base_path)
                        .arg("xzf")
                        .arg(LEDGER_STORAGE_FILE)
                        .kill_on_drop(true)
                        .spawn()
                        .map_err(|_| ReconcileError::StorageAcquireError)?;

                    let status = tar_child
                        .wait()
                        .await
                        .map_err(|_| ReconcileError::StorageAcquireError)?;

                    if !status.success() {
                        fail = true;
                    }
                }

                // unconditionally remove the tar regardless of success
                let _ = tokio::fs::remove_file(base_path.join(LEDGER_STORAGE_FILE)).await;

                // return an error if the storage acquisition failed
                if fail {
                    return Err(ReconcileError::StorageAcquireError);
                }
            }

            // reconcile towards new state
            match target.clone() {
                // do nothing on inventory state
                AgentState::Inventory => (),

                // start snarkOS node when node
                AgentState::Node(_, node) => {
                    let mut child_lock = state.child.write().await;
                    let mut command = Command::new(state.cli.path.join(SNARKOS_FILE));

                    command
                        // .kill_on_drop(true)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        // .stdin(Stdio::null())
                        .arg("--log")
                        .arg(state.cli.path.join(SNARKOS_LOG_FILE))
                        .arg("run")
                        .arg("--type")
                        .arg(node.ty.to_string())
                        // storage configuration
                        .arg("--genesis")
                        .arg(state.cli.path.join(SNARKOS_GENESIS_FILE))
                        .arg("--ledger")
                        .arg(state.cli.path.join(SNARKOS_LEDGER_DIR))
                        // port configuration
                        .arg("--bind")
                        .arg(state.cli.bind_addr.to_string())
                        .arg("--bft")
                        .arg(state.cli.bft.to_string())
                        .arg("--rest")
                        .arg(state.cli.rest.to_string())
                        .arg("--node")
                        .arg(state.cli.node.to_string());

                    if let Some(pk) = node.private_key {
                        command.arg("--private-key").arg(pk);
                    }

                    // Find agents that do not have cached addresses
                    let unresolved_addrs: HashSet<AgentId> = {
                        let resolved_addrs = state.resolved_addrs.read().await;
                        node.peers
                            .iter()
                            .chain(node.validators.iter())
                            .filter_map(|p| {
                                if let AgentPeer::Internal(id, _) = p {
                                    (!resolved_addrs.contains_key(id)).then_some(*id)
                                } else {
                                    None
                                }
                            })
                            .collect()
                    };

                    // Fetch all unresolved addresses and update the cache
                    if !unresolved_addrs.is_empty() {
                        tracing::debug!("need to resolve addrs: {unresolved_addrs:?}");
                        let new_addrs = state
                            .client
                            .resolve_addrs(context::current(), unresolved_addrs)
                            .await
                            .map_err(|err| {
                                error!("rpc error while resolving addresses: {err}");
                                ReconcileError::Unknown
                            })?
                            .map_err(ReconcileError::ResolveAddrError)?;
                        tracing::debug!("resolved new addrs: {new_addrs:?}");
                        state.resolved_addrs.write().await.extend(new_addrs);
                    }

                    if !node.peers.is_empty() {
                        command
                            .arg("--peers")
                            .arg(state.agentpeers_to_cli(&node.peers).await.join(","));
                    }

                    if !node.validators.is_empty() {
                        command
                            .arg("--validators")
                            .arg(state.agentpeers_to_cli(&node.validators).await.join(","));
                    }

                    if node.online {
                        tracing::trace!("spawning node process...");
                        tracing::debug!("node command: {command:?}");
                        let mut child = command.spawn().expect("failed to start child");

                        // start a new task to log stdout
                        // TODO: probably also want to read stderr
                        let stdout: tokio::process::ChildStdout = child.stdout.take().unwrap();
                        let stderr: tokio::process::ChildStderr = child.stderr.take().unwrap();

                        tokio::spawn(async move {
                            let child_span = tracing::span!(Level::INFO, "child process stdout");
                            let _enter = child_span.enter();

                            let mut reader1 = BufReader::new(stdout).lines();
                            let mut reader2 = BufReader::new(stderr).lines();

                            loop {
                                tokio::select! {
                                    Ok(line) = reader1.next_line() => {
                                        if let Some(line) = line {
                                            info!(line);
                                        } else {
                                            break;
                                        }
                                    }
                                    Ok(Some(line)) = reader2.next_line() => {
                                            error!(line);
                                    }
                                }
                            }
                        });

                        *child_lock = Some(child);

                        // todo: check to ensure the node actually comes online
                        // by hitting the REST latest block
                    } else {
                        tracing::debug!("skipping node spawn");
                    }
                }
            }

            // After completing the reconcilation, update the agent state
            let mut agent_state = state.agent_state.write().await;
            *agent_state = target;

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

    async fn get_addrs(self, _: context::Context) -> (PortConfig, Option<IpAddr>, Vec<IpAddr>) {
        (
            PortConfig {
                bft: self.state.cli.bft,
                node: self.state.cli.node,
                rest: self.state.cli.rest,
            },
            self.state.external_addr,
            self.state.internal_addrs.clone(),
        )
    }

    async fn get_state_root(self, _: context::Context) -> Result<String, AgentError> {
        if !matches!(
            self.state.agent_state.read().await.deref(),
            AgentState::Node(_, _)
        ) {
            return Err(AgentError::InvalidState);
        }

        let url = format!(
            "http://127.0.0.1:{}/mainnet/latest/stateRoot",
            self.state.cli.rest
        );
        let response = reqwest::get(&url)
            .await
            .map_err(|_| AgentError::FailedToMakeRequest)?;
        response
            .json()
            .await
            .map_err(|_| AgentError::FailedToParseJson)
    }
}
