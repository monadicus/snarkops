//! Control plane-to-agent RPC.

use std::{
    collections::HashSet, net::IpAddr, ops::Deref, path::PathBuf, process::Stdio, sync::Arc,
};

use snops_common::{
    aot_cmds::AotCmd,
    binaries::{BinaryEntry, BinarySource},
    constant::{
        LEDGER_BASE_DIR, LEDGER_PERSIST_DIR, SNARKOS_FILE, SNARKOS_GENESIS_FILE, SNARKOS_LOG_FILE,
    },
    define_rpc_mux,
    prelude::snarkos_status::SnarkOSLiteBlock,
    rpc::{
        control::{
            agent::{
                AgentMetric, AgentService, AgentServiceRequest, AgentServiceResponse, Handshake,
            },
            ControlServiceRequest, ControlServiceResponse,
        },
        error::{AgentError, ReconcileError, SnarkosRequestError},
    },
    state::{AgentId, AgentPeer, AgentState, EnvId, InternedId, KeyState, NetworkId, PortConfig},
};
use tarpc::context;
use tokio::process::Command;
use tracing::{debug, error, info, trace, warn};

use crate::{
    api, make_env_filter,
    metrics::MetricComputer,
    reconcile::{self, ensure_correct_binary},
    state::AppState,
};

define_rpc_mux!(child;
    ControlServiceRequest => ControlServiceResponse;
    AgentServiceRequest => AgentServiceResponse;
);

#[derive(Clone)]
pub struct AgentRpcServer {
    pub state: AppState,
}

impl AgentService for AgentRpcServer {
    async fn kill(self, _: context::Context) {
        self.state.node_graceful_shutdown().await;
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(1));
            std::process::exit(0)
        });
    }

    async fn handshake(
        self,
        context: context::Context,
        handshake: Handshake,
    ) -> Result<(), ReconcileError> {
        if let Some(token) = handshake.jwt {
            // cache the JWT in the state JWT mutex
            self.state
                .db
                .set_jwt(Some(token))
                .map_err(|_| ReconcileError::Database)?;
        }

        // store loki server URL
        if let Some(loki) = handshake.loki.and_then(|l| l.parse::<url::Url>().ok()) {
            self.state
                .loki
                .lock()
                .expect("failed to acquire loki URL lock")
                .replace(loki);
        }

        // emit the transfer statuses
        if let Err(err) = self
            .state
            .client
            .post_transfer_statuses(
                context,
                self.state
                    .transfers
                    .iter()
                    .map(|e| (*e.key(), e.value().clone()))
                    .collect(),
            )
            .await
        {
            error!("failed to send transfer statuses: {err}");
        }

        // reconcile if state has changed
        let needs_reconcile = *self.state.agent_state.read().await != handshake.state;
        if needs_reconcile {
            Self::reconcile(self, context, handshake.state).await?;
        }

        Ok(())
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
                        state.node_graceful_shutdown().await;
                    }

                    _ => (),
                }

                agent_state_lock.deref().clone()
            };

            // download new storage if storage_id changed
            'storage: {
                let (is_same_env, is_same_index) = match (&old_state, &target) {
                    (AgentState::Node(old_env, old_node), AgentState::Node(new_env, new_node)) => {
                        (old_env == new_env, old_node.height.0 == new_node.height.0)
                    }
                    _ => (false, false),
                };

                // skip if we don't need storage
                let AgentState::Node(env_id, node) = &target else {
                    break 'storage;
                };

                // get the storage info for this environment if we don't have it cached
                let info = state
                    .get_env_info(*env_id)
                    .await
                    .map_err(|_| ReconcileError::StorageAcquireError("storage info".to_owned()))?;

                // ensure the binary is correct every reconcile (or restart)
                ensure_correct_binary(node.binary, &state, &info).await?;

                if is_same_env && is_same_index {
                    debug!("skipping storage download");
                    break 'storage;
                }

                // TODO: download storage to a cache directory (~/config/.snops) to prevent
                // multiple agents from having to redownload
                // can be configurable to also work from a network drive

                // download and decompress the storage
                let height = &node.height.1;

                trace!("checking storage files...");

                // only download storage if it's a new environment
                // if a node starts at height: 0, the node will never
                // download the ledger
                if !is_same_env {
                    reconcile::check_files(&state, &info, height).await?;
                }
                reconcile::load_ledger(&state, &info, height, !is_same_env).await?;
                // TODO: checkpoint/absolute height request handling
            }

            // reconcile towards new state
            match target.clone() {
                // inventory state is waiting for a node to be started
                AgentState::Inventory => {
                    // wipe the env info cache. don't want to have stale storage info
                    state.env_info.write().await.take();
                }

                // start snarkOS node when node
                AgentState::Node(env_id, node) => {
                    let mut child_lock = state.child.write().await;
                    let mut command = Command::new(state.cli.path.join(SNARKOS_FILE));

                    // get the storage info for this environment if we don't have it cached
                    let info = state.get_env_info(env_id).await.map_err(|_| {
                        ReconcileError::StorageAcquireError("storage info".to_owned())
                    })?;

                    let storage_id = &info.storage.id;
                    let storage_path = state
                        .cli
                        .path
                        .join("storage")
                        .join(info.network.to_string())
                        .join(storage_id.to_string());
                    let ledger_path = if info.storage.persist {
                        storage_path.join(LEDGER_PERSIST_DIR)
                    } else {
                        state.cli.path.join(LEDGER_BASE_DIR)
                    };

                    // add loki URL if one is set
                    if let Some(loki) = &*state.loki.lock().unwrap() {
                        command
                            .env(
                                "SNOPS_LOKI_LABELS",
                                format!("env_id={},node_key={}", env_id, node.node_key),
                            )
                            .arg("--loki")
                            .arg(loki.as_str());
                    }

                    if state.cli.quiet {
                        command.stdout(Stdio::null());
                    } else {
                        command.stdout(std::io::stdout());
                    }

                    command
                        .stderr(std::io::stderr())
                        .envs(&node.env)
                        .env("NETWORK", info.network.to_string())
                        .env("HOME", &ledger_path)
                        .arg("--log")
                        .arg(state.cli.path.join(SNARKOS_LOG_FILE))
                        .arg("run")
                        .arg("--agent-rpc-port")
                        .arg(state.agent_rpc_port.to_string())
                        .arg("--type")
                        .arg(node.node_key.ty.to_string())
                        .arg("--ledger")
                        .arg(ledger_path);

                    if !info.storage.native_genesis {
                        command
                            .arg("--genesis")
                            .arg(storage_path.join(SNARKOS_GENESIS_FILE));
                    }

                    // storage configuration
                    command
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
                    if let Some(policy) = &info.storage.retention_policy {
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
                        tracing::debug!(
                            "need to resolve addrs: {}",
                            unresolved_addrs
                                .iter()
                                .map(|id| id.to_string())
                                .collect::<Vec<_>>()
                                .join(",")
                        );
                        let new_addrs = state
                            .client
                            .resolve_addrs(context::current(), unresolved_addrs)
                            .await
                            .map_err(|err| {
                                error!("rpc error while resolving addresses: {err}");
                                ReconcileError::Unknown
                            })?
                            .map_err(ReconcileError::ResolveAddrError)?;
                        tracing::debug!(
                            "resolved new addrs: {}",
                            new_addrs
                                .iter()
                                .map(|(id, addr)| format!("{}: {}", id, addr))
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
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
                        let child = command.spawn().expect("failed to start child");

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

    async fn snarkos_get(
        self,
        _: context::Context,
        route: String,
    ) -> Result<String, SnarkosRequestError> {
        let env_id =
            if let AgentState::Node(env_id, state) = self.state.agent_state.read().await.deref() {
                if !state.online {
                    return Err(SnarkosRequestError::OfflineNode);
                }
                *env_id
            } else {
                return Err(SnarkosRequestError::InvalidState);
            };

        let network = self
            .state
            .get_env_info(env_id)
            .await
            .map_err(|e| {
                error!("failed to get env info: {e}");
                SnarkosRequestError::MissingEnvInfo
            })?
            .network;

        let url = format!(
            "http://{}:{}/{network}{route}",
            self.state.cli.get_local_ip(),
            self.state.cli.ports.rest
        );
        let response = reqwest::get(&url)
            .await
            .map_err(|err| SnarkosRequestError::RequestError(err.to_string()))?;

        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|err| SnarkosRequestError::JsonParseError(err.to_string()))?;

        serde_json::to_string_pretty(&value)
            .map_err(|err| SnarkosRequestError::JsonSerializeError(err.to_string()))
    }

    async fn broadcast_tx(self, _: context::Context, tx: String) -> Result<(), AgentError> {
        let env_id =
            if let AgentState::Node(env_id, _) = self.state.agent_state.read().await.deref() {
                *env_id
            } else {
                return Err(AgentError::InvalidState);
            };

        let network = self
            .state
            .get_env_info(env_id)
            .await
            .map_err(|_| AgentError::FailedToMakeRequest)?
            .network;

        let url = format!(
            "http://{}:{}/{network}/transaction/broadcast",
            self.state.cli.get_local_ip(),
            self.state.cli.ports.rest
        );
        let response = reqwest::Client::new()
            .post(url)
            .header("Content-Type", "application/json")
            .body(tx)
            .send()
            .await
            .map_err(|_| AgentError::FailedToMakeRequest)?;
        let status = response.status();
        if status.is_success() {
            Ok(())
            // transaction already exists so this is technically a success
        } else if status.is_server_error()
            && response
                .text()
                .await
                .ok()
                .is_some_and(|text| text.contains("exists in the ledger"))
        {
            return Ok(());
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
        env_id: EnvId,
        network: NetworkId,
        query: String,
        auth: String,
    ) -> Result<String, AgentError> {
        info!("executing authorization...");

        // TODO: maybe in the env config store a branch label for the binary so it won't
        // be put in storage and won't overwrite itself

        let info = self
            .state
            .get_env_info(env_id)
            .await
            .map_err(|e| AgentError::FailedToGetEnvInfo(e.to_string()))?;

        let aot_bin = self
            .state
            .cli
            .path
            .join(format!("snarkos-aot-{env_id}-compute"));

        let default_entry = BinaryEntry {
            source: BinarySource::Path(PathBuf::from(format!(
                "/content/storage/{}/{}/binaries/default",
                info.network, info.storage.id,
            ))),
            sha256: None,
            size: None,
        };

        // download the snarkOS binary
        api::check_binary(
            // attempt to use the specified "compute" binary
            info.storage
                .binaries
                .get(&InternedId::compute_id())
                // fallback to the default binary
                .or_else(|| info.storage.binaries.get(&InternedId::default()))
                // fallback to the default entry
                .unwrap_or(&default_entry),
            &self.state.endpoint,
            &aot_bin,
            self.state.transfer_tx(),
        ) // TODO: http(s)?
        .await
        .map_err(|e| {
            error!("failed obtain runner binary: {e}");
            AgentError::ProcessFailed
        })?;

        let start = std::time::Instant::now();
        match AotCmd::new(aot_bin, network)
            .execute(
                serde_json::from_str(&auth).map_err(|_| AgentError::FailedToParseJson)?,
                format!("{}{query}", self.state.endpoint),
            )
            .await
        {
            Ok(exec) => {
                let elapsed = start.elapsed().as_millis();
                info!("authorization executed in {elapsed}ms");
                trace!("authorization output: {exec}");
                Ok(exec)
            }
            Err(e) => {
                error!("failed to execute: {e}");
                Err(AgentError::ProcessFailed)
            }
        }
    }

    async fn set_log_level(self, _: context::Context, level: String) -> Result<(), AgentError> {
        tracing::debug!("setting log level to {level}");
        let level: tracing_subscriber::filter::LevelFilter = level
            .parse()
            .map_err(|_| AgentError::InvalidLogLevel(level.clone()))?;
        self.state
            .log_level_handler
            .modify(|filter| *filter = make_env_filter(level))
            .map_err(|_| AgentError::FailedToChangeLogLevel)?;

        Ok(())
    }

    async fn set_aot_log_level(
        self,
        ctx: context::Context,
        verbosity: u8,
    ) -> Result<(), AgentError> {
        tracing::debug!("agent setting aot log verbosity to {verbosity:?}");
        let lock = self.state.node_client.lock().await;
        let node_client = lock.as_ref().ok_or(AgentError::NodeClientNotSet)?;
        node_client
            .set_log_level(ctx, verbosity)
            .await
            .map_err(|_| AgentError::FailedToChangeLogLevel)?
    }

    async fn get_snarkos_block_lite(
        self,
        ctx: context::Context,
        block_hash: String,
    ) -> Result<Option<SnarkOSLiteBlock>, AgentError> {
        let lock = self.state.node_client.lock().await;
        let node_client = lock.as_ref().ok_or(AgentError::NodeClientNotSet)?;
        node_client
            .get_block_lite(ctx, block_hash)
            .await
            .map_err(|_| AgentError::FailedToMakeRequest)?
    }

    async fn find_transaction(
        self,
        context: context::Context,
        tx_id: String,
    ) -> Result<Option<String>, AgentError> {
        let lock = self.state.node_client.lock().await;
        let node_client = lock.as_ref().ok_or(AgentError::NodeClientNotSet)?;
        node_client
            .find_transaction(context, tx_id)
            .await
            .map_err(|_| AgentError::FailedToMakeRequest)?
    }
}
