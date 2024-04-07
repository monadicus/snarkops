use std::{
    collections::HashSet, fs, net::IpAddr, ops::Deref, process::Stdio, sync::Arc, time::Duration,
};

use snops_common::{
    constant::{
        LEDGER_BASE_DIR, LEDGER_PERSIST_DIR, LEDGER_STORAGE_FILE, SNARKOS_FILE,
        SNARKOS_GENESIS_FILE, SNARKOS_LOG_FILE,
    },
    rpc::{
        agent::{AgentMetric, AgentService, AgentServiceRequest, AgentServiceResponse},
        control::{ControlServiceRequest, ControlServiceResponse},
        error::{AgentError, ReconcileError},
        MuxMessage,
    },
    state::{AgentId, AgentPeer, AgentState, KeyState, PortConfig},
};
use tarpc::{context, ClientMessage, Response};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    select,
};
use tracing::{debug, error, info, trace, warn, Level};

use crate::{api, metrics::MetricComputer, state::AppState};

/// The JWT file name.
pub const JWT_FILE: &str = "jwt";

pub const NODE_GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

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

                        if let Some((mut child, id)) =
                            state.child.write().await.take().and_then(|ch| {
                                let id = ch.id()?;
                                Some((ch, id))
                            })
                        {
                            use nix::{
                                sys::signal::{self, Signal},
                                unistd::Pid,
                            };

                            // send SIGINT to the child process
                            signal::kill(Pid::from_raw(id as i32), Signal::SIGINT).unwrap();

                            // wait for graceful shutdown or kill process after 10 seconds
                            let timeout = tokio::time::sleep(NODE_GRACEFUL_SHUTDOWN_TIMEOUT);

                            select! {
                                _ = child.wait() => (),
                                _ = timeout => {
                                    info!("snarkos process did not gracefully shut down, killing...");
                                    child.kill().await.unwrap();
                                }
                            }
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
                        // same environment id
                        // TODO: check if we need to update the ledger height
                        debug!("skipping agent storage download");
                        break 'storage;
                    }

                    _ => (),
                }

                // TODO: download storage to a cache directory

                // clean up old storage
                let base_path = &state.cli.path;
                let directories = &[base_path.join(LEDGER_BASE_DIR)];

                for dir in directories {
                    let _ = tokio::fs::remove_dir_all(dir).await;
                }

                // download and decompress the storage
                // skip if we don't need storage
                let AgentState::Node(env_id, _) = &target else {
                    info!("agent is not running a node; skipping storage download");
                    break 'storage;
                };

                // get the storage info for this environment if we don't have it cached
                let info = state
                    .get_env_info(*env_id)
                    .await
                    .map_err(|_| ReconcileError::StorageAcquireError)?;

                let storage_id = &info.id;
                let storage_path = base_path.join("storage").join(storage_id);

                // create the directory containing the storage files
                tokio::fs::create_dir_all(&storage_path)
                    .await
                    .map_err(|_| ReconcileError::StorageAcquireError)?;

                trace!("checking storage files...");

                let genesis_url = format!(
                    "http://{}/content/storage/{storage_id}/{SNARKOS_GENESIS_FILE}",
                    &state.endpoint
                );

                let ledger_url = format!(
                    "http://{}/content/storage/{storage_id}/{LEDGER_STORAGE_FILE}",
                    &state.endpoint
                );

                // download the snarkOS binary
                api::check_binary(
                    *env_id,
                    &format!("http://{}", &state.endpoint),
                    &base_path.join(SNARKOS_FILE),
                ) // TODO: http(s)?
                .await
                .expect("failed to acquire snarkOS binary");

                // download the genesis block
                api::check_file(genesis_url, &storage_path.join(SNARKOS_GENESIS_FILE))
                    .await
                    .map_err(|_| ReconcileError::StorageAcquireError)?;

                // download the ledger file
                api::check_file(ledger_url, &storage_path.join(LEDGER_STORAGE_FILE))
                    .await
                    .map_err(|_| ReconcileError::StorageAcquireError)?;

                // use a persisted directory for the untar when configured
                let (untar_base, untar_dir) = if target.is_persist() {
                    if fs::metadata(storage_path.join(LEDGER_PERSIST_DIR)).is_ok() {
                        info!("persisted ledger already exists for {storage_id}");
                        break 'storage;
                    }

                    info!("using persisted ledger for {storage_id}");

                    (&storage_path, LEDGER_PERSIST_DIR)
                } else {
                    info!("using fresh ledger for {storage_id}");
                    (base_path, LEDGER_BASE_DIR)
                };

                tokio::fs::create_dir_all(&untar_base.join(untar_dir))
                    .await
                    .map_err(|_| ReconcileError::StorageAcquireError)?;

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
                    .map_err(|_| ReconcileError::StorageAcquireError)?
                    .wait()
                    .await
                    .map_err(|_| ReconcileError::StorageAcquireError)?;

                if !status.success() {
                    return Err(ReconcileError::StorageAcquireError);
                }
            }

            // reconcile towards new state
            match target.clone() {
                // do nothing on inventory state
                AgentState::Inventory => (),

                // start snarkOS node when node
                AgentState::Node(env_id, node) => {
                    let mut child_lock = state.child.write().await;
                    let mut command = Command::new(state.cli.path.join(SNARKOS_FILE));

                    // get the storage info for this environment if we don't have it cached
                    let info = state
                        .get_env_info(env_id)
                        .await
                        .map_err(|_| ReconcileError::StorageAcquireError)?;

                    let storage_id = &info.id;
                    let storage_path = state.cli.path.join("storage").join(storage_id);
                    let ledger_path = if target.is_persist() {
                        storage_path.join(LEDGER_PERSIST_DIR)
                    } else {
                        state.cli.path.join(LEDGER_BASE_DIR)
                    };

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
                        .arg(storage_path.join(SNARKOS_GENESIS_FILE))
                        .arg("--ledger")
                        .arg(ledger_path)
                        // port configuration
                        .arg("--bind")
                        .arg(state.cli.bind_addr.to_string())
                        .arg("--bft")
                        .arg(state.cli.ports.bft.to_string())
                        .arg("--rest")
                        .arg(state.cli.ports.rest.to_string())
                        .arg("--metrics")
                        .arg(state.cli.ports.metrics.to_string())
                        .arg("--node")
                        .arg(state.cli.ports.node.to_string());

                    match node.private_key {
                        KeyState::None => {}
                        KeyState::Local => {
                            command.arg("--private-key-file").arg(
                                state
                                    .cli
                                    .private_key_file
                                    .as_ref()
                                    .ok_or(ReconcileError::NoLocalPrivateKey)?,
                            );
                        }
                        KeyState::Literal(pk) => {
                            command.arg("--private-key").arg(pk);
                        }
                    }

                    // conditionally add retention policy
                    if let Some(policy) = &info.retention_policy {
                        command.arg("--retention-policy").arg(policy.to_string());
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
            self.state.cli.ports.clone(),
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
            self.state.cli.ports.rest
        );
        let response = reqwest::get(&url)
            .await
            .map_err(|_| AgentError::FailedToMakeRequest)?;
        response
            .json()
            .await
            .map_err(|_| AgentError::FailedToParseJson)
    }

    async fn broadcast_tx(self, _: context::Context, tx: String) -> Result<(), AgentError> {
        if !matches!(
            self.state.agent_state.read().await.deref(),
            AgentState::Node(_, _)
        ) {
            return Err(AgentError::InvalidState);
        }

        let url = format!(
            "http://127.0.0.1:{}/mainnet/transaction/broadcast",
            self.state.cli.ports.rest
        );
        let response = reqwest::Client::new()
            .post(url)
            .header("Content-Type", "application/json")
            .body(tx)
            .send()
            .await
            .map_err(|_| AgentError::FailedToMakeRequest)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(AgentError::FailedToMakeRequest)
        }
    }

    async fn get_metric(self, _: context::Context, metric: AgentMetric) -> f64 {
        let metrics = self.state.metrics.read().await;

        match metric {
            AgentMetric::Tps => metrics.tps.get(),
        }
    }

    async fn execute_authorization(
        self,
        _: context::Context,
        env_id: usize,
        query: String,
        auth: String,
    ) -> Result<(), AgentError> {
        info!("executing authorization...");

        // TODO: maybe in the env config store a branch label for the binary so it won't be put in storage and won't overwrite itself

        // download the snarkOS binary
        api::check_binary(
            env_id,
            &format!("http://{}", &self.state.endpoint),
            &self.state.cli.path.join(SNARKOS_FILE),
        ) // TODO: http(s)?
        .await
        .expect("failed to acquire snarkOS binary");

        let res = Command::new(dbg!(self.state.cli.path.join(SNARKOS_FILE)))
            .stdout(std::io::stdout())
            .stderr(std::io::stderr())
            .arg("execute")
            .arg("--query")
            .arg(&format!("http://{}{query}", self.state.endpoint))
            .arg(auth)
            .spawn()
            .map_err(|e| {
                warn!("failed to spawn auth exec process: {e}");
                AgentError::FailedToSpawnProcess
            })?
            .wait()
            .await
            .map_err(|e| {
                warn!("auth exec process failed: {e}");
                AgentError::ProcessFailed
            })?;

        if !res.success() {
            warn!("auth exec process exited with status: {res}");
            return Err(AgentError::ProcessFailed);
        }
        Ok(())
    }
}
