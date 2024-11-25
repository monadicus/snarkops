//! Control plane-to-agent RPC.

use std::{net::IpAddr, path::PathBuf};

use snops_common::{
    aot_cmds::AotCmd,
    binaries::{BinaryEntry, BinarySource},
    define_rpc_mux,
    prelude::snarkos_status::SnarkOSLiteBlock,
    rpc::{
        control::{
            agent::{
                AgentMetric, AgentService, AgentServiceRequest, AgentServiceResponse, AgentStatus,
                Handshake,
            },
            ControlServiceClient, ControlServiceRequest, ControlServiceResponse,
        },
        error::{AgentError, ReconcileError, SnarkosRequestError},
    },
    state::{AgentState, EnvId, InternedId, NetworkId, PortConfig},
};
use tarpc::context;
use tracing::{error, info, trace};

use crate::{api, log::make_env_filter, metrics::MetricComputer, state::AppState};

define_rpc_mux!(child;
    ControlServiceRequest => ControlServiceResponse;
    AgentServiceRequest => AgentServiceResponse;
);

#[derive(Clone)]
pub struct AgentRpcServer {
    pub client: ControlServiceClient,
    pub state: AppState,
    pub version: &'static str,
}

impl AgentService for AgentRpcServer {
    async fn kill(self, _: context::Context) {
        info!("Kill RPC invoked...");
        self.state.shutdown().await;
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
        let loki_url = handshake.loki.and_then(|l| l.parse::<url::Url>().ok());

        if let Err(e) = self
            .state
            .db
            .set_loki_url(loki_url.as_ref().map(|u| u.to_string()))
        {
            error!("failed to save loki URL to db: {e}");
        }
        match self.state.loki.lock() {
            Ok(mut guard) => {
                *guard = loki_url;
            }
            Err(e) => {
                error!("failed to acquire loki URL lock: {e}");
            }
        }

        // emit the transfer statuses
        if let Err(err) = self
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

        info!("Received control-plane handshake");

        // Queue a reconcile immediately as we have received new state.
        // The reconciler will decide if anything has actually changed
        self.state.update_agent_state(handshake.state).await;

        Ok(())
    }

    async fn reconcile(
        self,
        _: context::Context,
        target: AgentState,
    ) -> Result<(), ReconcileError> {
        info!("Received reconcile request...");
        self.state.update_agent_state(target).await;
        Ok(())
    }

    async fn get_addrs(self, _: context::Context) -> (PortConfig, Option<IpAddr>, Vec<IpAddr>) {
        (
            self.state.cli.ports,
            self.state.external_addr,
            self.state.internal_addrs.clone(),
        )
    }

    async fn snarkos_get(
        self,
        _: context::Context,
        route: String,
    ) -> Result<String, SnarkosRequestError> {
        self.state
            .get_node_client()
            .await
            .ok_or(SnarkosRequestError::OfflineNode)?;

        let env_id =
            if let AgentState::Node(env_id, state) = self.state.agent_state.read().await.as_ref() {
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
        self.state
            .get_node_client()
            .await
            .ok_or(AgentError::NodeClientNotReady)?;

        let env_id =
            if let AgentState::Node(env_id, _) = self.state.agent_state.read().await.as_ref() {
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
        self.state
            .get_node_client()
            .await
            .ok_or(AgentError::NodeClientNotSet)?
            .set_log_level(ctx, verbosity)
            .await
            .map_err(|_| AgentError::FailedToChangeLogLevel)?
    }

    async fn get_snarkos_block_lite(
        self,
        ctx: context::Context,
        block_hash: String,
    ) -> Result<Option<SnarkOSLiteBlock>, AgentError> {
        self.state
            .get_node_client()
            .await
            .ok_or(AgentError::NodeClientNotSet)?
            .get_block_lite(ctx, block_hash)
            .await
            .map_err(|_| AgentError::FailedToMakeRequest)?
    }

    async fn find_transaction(
        self,
        context: context::Context,
        tx_id: String,
    ) -> Result<Option<String>, AgentError> {
        self.state
            .get_node_client()
            .await
            .ok_or(AgentError::NodeClientNotSet)?
            .find_transaction(context, tx_id)
            .await
            .map_err(|_| AgentError::FailedToMakeRequest)?
    }

    async fn get_status(self, ctx: context::Context) -> Result<AgentStatus, AgentError> {
        Ok(AgentStatus {
            aot_online: self
                .state
                .get_node_client()
                .await
                .ok_or(AgentError::NodeClientNotSet)?
                .status(ctx)
                .await
                .is_ok(),
            version: self.version.to_string(),
        })
    }
}
