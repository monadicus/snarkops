use std::sync::{atomic::AtomicUsize, Arc};

use chrono::Utc;
use dashmap::DashMap;
use futures_util::{stream::FuturesUnordered, StreamExt};
use lazysort::SortedBy;
use snops_common::{
    aot_cmds::Authorization,
    state::{CannonId, EnvId, NetworkId},
};
use tracing::{error, trace, warn};

use super::{
    error::{CannonError, ExecutionContextError, SourceError},
    file::TransactionSink,
    sink::TxSink,
    source::TxSource,
    status::{TransactionSendState, TransactionStatusEvent, TransactionStatusSender},
    tracker::TransactionTracker,
    CannonReceivers,
};
use crate::{
    cannon::source::ComputeTarget,
    state::{GlobalState, REST_CLIENT},
};

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
    pub(crate) transactions: Arc<DashMap<String, TransactionTracker>>,
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
                Some((tx_id, events)) = rx.authorizations.recv() => {
                    // ensure the transaction tracker exists
                    let Some(tracker) = self.transactions.get(&tx_id) else {
                        error!("cannon {env_id}.{cannon_id} missing transaction tracker for {tx_id}");
                        events.send(TransactionStatusEvent::ExecuteAborted);
                        continue;
                    };
                    // ensure the transaction is in the correct state
                    if tracker.status != TransactionSendState::Authorized {
                        error!("cannon {env_id}.{cannon_id} unexpected status for {tx_id}: {:?}", tracker.status);
                        // TODO: remove this auth and log it somewhere
                        events.send(TransactionStatusEvent::ExecuteAborted);
                        continue;
                    }
                    // ensure the transaction has an authorization (more than likely unreachable)
                    let Some(auth) = &tracker.authorization else {
                        error!("cannon {env_id}.{cannon_id} missing authorization for {tx_id}");
                        // TODO: remove the auth anyway
                        events.send(TransactionStatusEvent::ExecuteAborted);
                        continue;
                    };

                    auth_execs.push(self.execute_auth(tx_id, Arc::clone(auth), &query_path, events));
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
    pub fn write_tx_status(&self, tx_id: &str, status: TransactionSendState) {
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

    pub fn remove_tx_tracker(&self, tx_id: String) {
        let _ = self.transactions.remove(&tx_id);
        if let Err(e) =
            TransactionTracker::delete(&self.state, &(self.env_id, self.id, tx_id.to_owned()))
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
        tx_id: String,
        auth: Arc<Authorization>,
        query_path: &str,
        events: TransactionStatusSender,
    ) -> Result<(), (String, CannonError)> {
        events.send(TransactionStatusEvent::ExecuteQueued);
        match self
            .source
            .compute
            .execute(self, query_path, &tx_id, &auth, &events)
            .await
        {
            // Can't execute the auth if no agents are available.
            // The transaction task will handle re-appending the auth.
            Err(CannonError::Source(SourceError::NoAvailableAgents(_))) => {
                events.send(TransactionStatusEvent::ExecuteAwaitingCompute);
                Ok(())
            }
            Err(e) => {
                // reset the transaction status to authorized so it can be re-executed
                self.write_tx_status(&tx_id, TransactionSendState::Authorized);
                events.send(TransactionStatusEvent::ExecuteFailed(e.to_string()));
                Err((tx_id, e))
            }
            res => res.map_err(|e| (tx_id, e)),
        }
    }

    /// Fire a transaction to the sink
    async fn fire_tx(
        &self,
        sink_pipe: Option<Arc<TransactionSink>>,
        tx_id: String,
    ) -> Result<String, CannonError> {
        let latest_height = self
            .state
            .get_env_block_info(self.env_id)
            .map(|info| info.height);

        // ensure transaction is being tracked
        let Some(tracker) = self.transactions.get(&tx_id).map(|v| v.value().clone()) else {
            return Err(CannonError::TransactionLost(self.id, tx_id));
        };
        // ensure transaction is ready to be broadcasted
        if !matches!(
            tracker.status,
            TransactionSendState::Unsent | TransactionSendState::Broadcasted(_, _)
        ) {
            return Err(CannonError::InvalidTransactionState(
                self.id,
                tx_id,
                format!(
                    "expected unsent or broadcasted, got {}",
                    tracker.status.label()
                ),
            ));
        }

        // ensure transaction blob exists
        let Some(tx_blob) = tracker.transaction else {
            return Err(CannonError::TransactionLost(self.id, tx_id));
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
            let update_status = || {
                self.write_tx_status(
                    &tx_id,
                    TransactionSendState::Broadcasted(latest_height, Utc::now()),
                );
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

                    update_status();
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
                            if !req.status().is_success() {
                                warn!("cannon {env_id}.{cannon_id} failed to broadcast transaction to {addr}: {}", req.status());
                                continue;
                            }
                        }
                    }

                    update_status();
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
