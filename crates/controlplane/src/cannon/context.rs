use std::sync::{atomic::AtomicUsize, Arc};

use chrono::Utc;
use dashmap::DashMap;
use futures_util::{stream::FuturesUnordered, StreamExt};
use lazysort::SortedBy;
use snops_common::{
    events::{Event, TransactionAbortReason, TransactionEvent},
    schema::cannon::{
        sink::TxSink,
        source::{ComputeTarget, TxSource},
    },
    state::{AgentId, Authorization, CannonId, EnvId, NetworkId, TransactionSendState},
};
use tracing::{error, trace, warn};

use super::{
    error::{CannonError, ExecutionContextError, SourceError},
    file::TransactionSink,
    source::ExecuteAuth,
    tracker::TransactionTracker,
    CannonReceivers,
};
use crate::state::{EmitEvent, GetGlobalState, GlobalState, REST_CLIENT};

/// Information a transaction cannon needs for execution via spawned task
pub struct ExecutionContext {
    pub(crate) state: Arc<GlobalState>,
    /// The cannon's id
    pub(crate) id: CannonId,
    /// The environment associated with this cannon
    pub(crate) env_id: EnvId,
    pub(crate) network: NetworkId,
    pub(crate) source: TxSource,
    pub(crate) sink: TxSink,
    pub(crate) fired_txs: Arc<AtomicUsize>,
    pub(crate) transactions: Arc<DashMap<Arc<String>, TransactionTracker>>,
}

impl ExecutionContext {
    pub async fn spawn(self, mut rx: CannonReceivers) -> Result<(), CannonError> {
        let ExecutionContext {
            id: cannon_id,
            env_id,
            source,
            sink,
            fired_txs,
            state,
            ..
        } = &self;

        let env_id = *env_id;
        let env = state
            .get_env(env_id)
            .ok_or_else(|| ExecutionContextError::EnvDropped(env_id, *cannon_id))?;

        trace!("cannon {env_id}.{cannon_id} spawned");

        // get the query path from the realtime tx source
        let suffix = format!("/api/v1/env/{}/cannons/{cannon_id}", env.id);
        let query_path = match source.compute {
            // agents already know the host of the control plane
            ComputeTarget::Agent { .. } => suffix,
            // demox needs to locate it
            ComputeTarget::Demox { .. } => {
                let host = state
                    .cli
                    .hostname
                    .as_ref()
                    .ok_or(ExecutionContextError::NoHostnameConfigured)?;
                format!("{host}:{}{suffix}", state.cli.port)
            }
        };
        trace!("cannon {env_id}.{cannon_id} using realtime query {query_path}");

        let sink_pipe = sink
            .file_name
            .map(|file_name| {
                env.sinks.get(&file_name).cloned().ok_or_else(|| {
                    ExecutionContextError::TransactionSinkNotFound(env_id, *cannon_id, file_name)
                })
            })
            .transpose()?;

        let mut auth_execs = FuturesUnordered::new();
        let mut tx_shots = FuturesUnordered::new();

        loop {
            tokio::select! {
                // ------------------------
                // Work generation
                // ------------------------

                // receive authorizations and forward the executions to the compute target
                Some(tx_id) = rx.authorizations.recv() => {
                    // ensure the transaction tracker exists
                    let Some(tracker) = self.transactions.get(&tx_id) else {
                        error!("cannon {env_id}.{cannon_id} missing transaction tracker for {tx_id}");
                        TransactionEvent::ExecuteAborted(TransactionAbortReason::MissingTracker).with_cannon_ctx(&self, tx_id).emit(&self);
                        continue;
                    };
                    // ensure the transaction is in the correct state
                    if tracker.status != TransactionSendState::Authorized {
                        error!("cannon {env_id}.{cannon_id} unexpected status for {tx_id}: {:?}", tracker.status);
                        // TODO: remove this auth and log it somewhere
                        TransactionEvent::ExecuteAborted(TransactionAbortReason::UnexpectedStatus{ transaction_status: tracker.status}).with_cannon_ctx(&self, tx_id).emit(&self);
                        continue;
                    }
                    // ensure the transaction has an authorization (more than likely unreachable)
                    let Some(auth) = &tracker.authorization else {
                        error!("cannon {env_id}.{cannon_id} missing authorization for {tx_id}");
                        // TODO: remove the auth anyway
                        TransactionEvent::ExecuteAborted(TransactionAbortReason::MissingAuthorization).with_cannon_ctx(&self, tx_id).emit(&self);
                        continue;
                    };

                    auth_execs.push(self.execute_auth(tx_id, Arc::clone(auth), &query_path));
                }
                // receive transaction ids and forward them to the sink target
                Some(tx) = rx.transactions.recv() => {
                    tx_shots.push(self.fire_tx(sink_pipe.clone(), tx));
                }

                // ------------------------
                // Work results
                // ------------------------

                Some(res) = auth_execs.next() => {
                    if let Err((tx_id, e)) = res {
                        warn!("cannon {env_id}.{cannon_id} auth execute task {tx_id} failed: {e}");
                    }
                },
                Some(res) = tx_shots.next() => {
                    match res {
                        Ok(tx_id) => {
                            let _fired_count = fired_txs.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                            trace!("cannon {env_id}.{cannon_id} broadcasted {tx_id}");
                        }
                        Err(e) => {
                            warn!("cannon {env_id}.{cannon_id} failed to fire transaction {e}");
                        }
                    }
                },
            }
        }
    }

    // write the transaction status to the store and update the transaction tracker
    pub fn write_tx_status(&self, tx_id: &Arc<String>, status: TransactionSendState) {
        let key = (self.env_id, self.id, tx_id.to_owned());
        if let Some(mut tx) = self.transactions.get_mut(tx_id) {
            if let Err(e) = TransactionTracker::write_status(&self.state, &key, status) {
                error!(
                    "cannon {}.{} failed to write status for {tx_id}: {e}",
                    self.env_id, self.id
                );
            }
            tx.status = status;
        }
    }

    pub fn remove_tx_tracker(&self, tx_id: Arc<String>) {
        let _ = self.transactions.remove(&tx_id);
        if let Err(e) =
            TransactionTracker::delete(&self.state, &(self.env_id, self.id, tx_id.clone()))
        {
            error!(
                "cannon {}.{} failed to delete transaction {tx_id}: {e:?}",
                self.env_id, self.id
            );
        }
    }

    /// Execute an authorization on the source's compute target
    async fn execute_auth(
        &self,
        tx_id: Arc<String>,
        auth: Arc<Authorization>,
        query_path: &str,
    ) -> Result<(), (Arc<String>, CannonError)> {
        TransactionEvent::AuthorizationReceived {
            authorization: Arc::clone(&auth),
        }
        .with_cannon_ctx(self, tx_id.clone())
        .emit(self);
        match self
            .source
            .compute
            .execute(self, query_path, &tx_id, &auth)
            .await
        {
            // Can't execute the auth if no agents are available.
            // The transaction task will handle re-appending the auth.
            Err(CannonError::Source(SourceError::NoAvailableAgents(_))) => {
                TransactionEvent::ExecuteAwaitingCompute
                    .with_cannon_ctx(self, tx_id.clone())
                    .emit(self);
                Ok(())
            }
            Err(e) => {
                // reset the transaction status to authorized so it can be re-executed
                self.write_tx_status(&tx_id, TransactionSendState::Authorized);
                TransactionEvent::ExecuteFailed(e.to_string())
                    .with_cannon_ctx(self, tx_id.clone())
                    .emit(self);
                Err((tx_id, e))
            }
            res => res.map_err(|e| (tx_id, e)),
        }
    }

    /// Fire a transaction to the sink
    async fn fire_tx(
        &self,
        sink_pipe: Option<Arc<TransactionSink>>,
        tx_id: Arc<String>,
    ) -> Result<Arc<String>, CannonError> {
        let latest_height = self
            .state
            .get_env_block_info(self.env_id)
            .map(|info| info.height);

        // ensure transaction is being tracked
        let Some(tracker) = self.transactions.get(&tx_id).map(|v| v.value().clone()) else {
            return Err(CannonError::TransactionLost(self.id, tx_id.to_string()));
        };
        // ensure transaction is ready to be broadcasted
        if !matches!(
            tracker.status,
            TransactionSendState::Unsent | TransactionSendState::Broadcasted(_, _)
        ) {
            return Err(CannonError::InvalidTransactionState(
                self.id,
                tx_id.to_string(),
                format!(
                    "expected unsent or broadcasted, got {}",
                    tracker.status.label()
                ),
            ));
        }

        // ensure transaction blob exists
        let Some(tx_blob) = tracker.transaction else {
            return Err(CannonError::TransactionLost(self.id, tx_id.to_string()));
        };

        let tx_str = match serde_json::to_string(&tx_blob) {
            Ok(tx_str) => tx_str,
            Err(e) => {
                return Err(CannonError::Source(SourceError::Json(
                    "serialize tx for broadcast",
                    e,
                )));
            }
        };

        if let Some(pipe) = sink_pipe {
            pipe.write(&tx_str)?;
        }

        let cannon_id = self.id;
        let env_id = self.env_id;

        if let Some(target) = &self.sink.target {
            let broadcast_nodes = self.state.get_scored_peers(env_id, target);

            if broadcast_nodes.is_empty() {
                return Err(ExecutionContextError::NoAvailableAgents(
                    env_id,
                    cannon_id,
                    "to broadcast transactions",
                )
                .into());
            }

            let network = self.network;

            // update the transaction status and increment the broadcast attempts
            let update_status = |agent: Option<AgentId>| {
                self.write_tx_status(
                    &tx_id,
                    TransactionSendState::Broadcasted(latest_height, Utc::now()),
                );
                let mut ev = TransactionEvent::Broadcasted {
                    height: latest_height,
                    timestamp: Utc::now(),
                }
                .with_cannon_ctx(self, Arc::clone(&tx_id));
                ev.agent = agent;
                ev.emit(self);

                if let Err(e) = TransactionTracker::inc_attempts(
                    &self.state,
                    &(env_id, cannon_id, tx_id.to_owned()),
                ) {
                    error!(
                        "cannon {env_id}.{cannon_id} failed to increment broadcast attempts for {tx_id}: {e}",
                    );
                }
            };

            // broadcast to the first responding node
            for (_, _, agent, addr) in broadcast_nodes.into_iter().sorted_by(|a, b| a.0.cmp(&b.0)) {
                if let Some(id) = agent {
                    // ensure the client is connected
                    let Some(client) = self.state.get_client(id) else {
                        continue;
                    };

                    if let Err(e) = client.broadcast_tx(tx_str.clone()).await {
                        warn!("cannon {env_id}.{cannon_id} failed to broadcast transaction to agent {id}: {e}");
                        continue;
                    }

                    update_status(agent);
                    return Ok(tx_id);
                }

                if let Some(addr) = addr {
                    let url = format!("http://{addr}/{network}/transaction/broadcast");
                    let req = REST_CLIENT
                        .post(url)
                        .header("Content-Type", "application/json")
                        .body(tx_str.clone())
                        .send();
                    let Ok(res) =
                        tokio::time::timeout(std::time::Duration::from_secs(5), req).await
                    else {
                        warn!("cannon {env_id}.{cannon_id} failed to broadcast transaction to {addr}: timeout");
                        continue;
                    };

                    match res {
                        Err(e) => {
                            warn!("cannon {env_id}.{cannon_id} failed to broadcast transaction to {addr}: {e}");
                            continue;
                        }
                        Ok(req) => {
                            let status = req.status();
                            if !status.is_success() {
                                // transaction already exists in the ledger but we'll confirm it
                                // anyway
                                if status.is_server_error()
                                    && req
                                        .text()
                                        .await
                                        .ok()
                                        .is_some_and(|text| text.contains("exists in the ledger"))
                                {
                                    return Ok(tx_id);
                                }

                                warn!("cannon {env_id}.{cannon_id} failed to broadcast transaction to {addr}: {}", status);
                                continue;
                            }
                        }
                    }

                    update_status(None);
                    return Ok(tx_id);
                }
            }

            Err(ExecutionContextError::NoAvailableAgents(
                env_id,
                cannon_id,
                "to broadcast transactions",
            ))?
        } else {
            // remove the transaction from the store as there is no need to
            // confirm the broadcast
            self.remove_tx_tracker(tx_id.clone());
        }
        Ok(tx_id)
    }
}

impl<'a> GetGlobalState<'a> for &'a ExecutionContext {
    fn global_state(self) -> &'a GlobalState {
        &self.state
    }
}

pub trait CtxEventHelper {
    fn with_cannon_ctx(self, ctx: &ExecutionContext, transaction: Arc<String>) -> Event;
}

impl<T: Into<Event>> CtxEventHelper for T {
    fn with_cannon_ctx(self, ctx: &ExecutionContext, transaction: Arc<String>) -> Event {
        let mut event = self.into();
        event.cannon = Some(ctx.id);
        event.env = Some(ctx.env_id);
        event.transaction = Some(transaction);
        event
    }
}
