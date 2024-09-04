use std::{sync::Arc, time::Duration};

use chrono::{TimeDelta, Utc};
use futures_util::future;
use snops_common::state::{CannonId, EnvId};
use tokio::time::timeout;
use tracing::{info, trace};

use super::GlobalState;
use crate::cannon::{
    status::{TransactionSendState, TransactionStatusSender},
    tracker::TransactionTracker,
};

/// This task re-sends all transactions that have not been confirmed,
/// re-computes all transactions that have not been computed, and removes
/// transactions that are confirmed.
pub async fn tracking_task(state: Arc<GlobalState>) {
    loop {
        let pending_txs = get_pending_transactions(&state);

        future::join_all(pending_txs.into_iter().map(|((env_id, cannon_id), pending)| {
            let state = state.clone();
            async move {
                let Some(env) = state.get_env(env_id) else {
                    return
                };
                let Some(cannon) = env.get_cannon(cannon_id) else {
                    return
                };

                // queue up all the transactions that need to be executed
                for tx_id in pending.to_execute {
                    if let Err(e) = cannon
                        .auth_sender
                        .send((tx_id.clone(), TransactionStatusSender::empty()))
                    {
                        tracing::error!(
                            "cannon {env_id}.{cannon_id} failed to send auth {tx_id} to cannon: {e:?}"
                        );
                    }
                }

                // queue up all the transactions that need to be confirmed
                for tx_id in pending.to_broadcast {
                    trace!("cannon {env_id}.{cannon_id} queueing transaction {tx_id} for re-broadcast");
                    if let Err(e) = cannon.tx_sender.send(tx_id.clone()) {
                        tracing::error!(
                            "cannon {env_id}.{cannon_id} failed to send broadcast {tx_id} to cannon: {e:?}"
                        );
                    }
                }

                // attempt to confirm all the confirm-pending transactions by using the cache
                // then fall back on making a request to the peers
                let confirmed = future::join_all(pending.to_confirm.into_iter().map(|(tx_id, _height)| {
                    let state = state.clone();
                    let cannon_target = cannon.sink.target.as_ref();
                    async move {
                        if let Some(cache) = state.env_network_cache.get(&env_id) {
                            if cache.has_transaction(&tx_id) {
                                trace!("cannon {env_id}.{cannon_id} confirmed transaction {tx_id} (cache hit)");
                                return Some(tx_id);
                            }
                        }

                        // check if the transaction not is in the cache, then check the peers
                        if let Some(target) = cannon_target {
                            match timeout(Duration::from_secs(1),
                            state.snarkos_get::<Option<String>>(env_id, format!("/find/blockHash/{tx_id}"), target)).await {
                                Ok(Ok(Some(_hash))) => {
                                    trace!("cannon {env_id}.{cannon_id} confirmed transaction {tx_id} (get request)");
                                    return Some(tx_id)
                                }
                                Ok(Ok(None)) => {
                                    // the transaction is not in the cache
                                }
                                _ => {}
                            }

                        }

                        None
                }})).await;

                // remove all the transactions that are confirmed or expired
                for tx_id in pending.to_remove.into_iter().chain(confirmed.into_iter().flatten()) {
                    cannon.transactions.remove(&tx_id);
                    if let Err(e) =
                        TransactionTracker::delete(&state, &(env_id, cannon_id, tx_id.clone()))
                    {
                        tracing::error!("cannon {env_id}.{cannon_id} failed to delete {tx_id}: {e:?}");
                    }
                }
            }})).await;

        // wait for the next update
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

struct PendingTransactions {
    to_execute: Vec<String>,
    to_broadcast: Vec<String>,
    to_remove: Vec<String>,
    to_confirm: Vec<(String, Option<u32>)>,
}

/// Get a list of transactions that need to be executed, broadcasted, removed,
/// or confirmed
fn get_pending_transactions(state: &GlobalState) -> Vec<((EnvId, CannonId), PendingTransactions)> {
    let now = Utc::now();
    let mut pending = vec![];

    for env in &state.envs {
        let env_id = *env.key();
        let latest_height = state.get_env_block_info(env_id).map(|b| b.height);

        for (cannon_id, cannon) in &env.cannons {
            let cannon_id = *cannon_id;
            let mut to_execute = vec![];
            let mut to_broadcast = vec![];
            let mut to_remove = vec![];
            let mut to_confirm = vec![];

            for tx in cannon.transactions.iter() {
                let tx_id = tx.key().to_owned();
                let key = (env_id, cannon_id, tx_id.to_owned());
                let attempts = TransactionTracker::get_attempts(state, &key);

                match tx.status {
                    // any authorized transaction that is not started should be queued
                    TransactionSendState::Authorized => {
                        if cannon.sink.authorize_attempts.is_some_and(|a| attempts > a) {
                            info!("cannon {env_id}.{cannon_id} removed auth {tx_id} (too many attempts)");
                            to_remove.push(tx_id);
                        } else {
                            to_execute.push((tx_id, tx.index));
                        }
                    }
                    // any expired execution should be queued
                    TransactionSendState::Executing(start_time)
                        if now - start_time
                            > TimeDelta::seconds(cannon.sink.authorize_timeout as i64) =>
                    {
                        if cannon.sink.authorize_attempts.is_some_and(|a| attempts > a) {
                            info!("cannon {env_id}.{cannon_id} removed auth {tx_id} (too many attempts)");
                            to_remove.push(tx_id);
                        } else {
                            to_execute.push((tx_id, tx.index));
                        }
                    }
                    // any unbroadcasted transaction that is not started should be queued
                    TransactionSendState::Unsent => {
                        if cannon.sink.broadcast_attempts.is_some_and(|a| attempts > a) {
                            info!("cannon {env_id}.{cannon_id} removed broadcast {tx_id} (too many attempts)");
                            to_remove.push(tx_id);
                        } else {
                            to_broadcast.push((tx_id, tx.index));
                        }
                    }
                    // any expired broadcast should be queued
                    // any broadcast that has a different height than the latest height
                    // should be confirmed
                    TransactionSendState::Broadcasted(height, broadcast_time) => {
                        // queue a confirm if the latest height is greater than the broadcast height
                        // or the broadcast height is unknown
                        //
                        // this feature is skipped if the sink has no node target
                        if cannon.sink.target.is_some()
                            && height
                                .map(|height| latest_height.is_some_and(|h| h > height))
                                .unwrap_or(true)
                        {
                            to_confirm.push(((tx_id.clone(), height), tx.index));
                        }

                        // queue a re-broadcast if the broadcast has timed out
                        if now - broadcast_time
                            > TimeDelta::seconds(cannon.sink.broadcast_timeout as i64)
                        {
                            if cannon.sink.broadcast_attempts.is_some_and(|a| attempts > a) {
                                info!("cannon {env_id}.{cannon_id} removed broadcast {tx_id} (too many attempts)");
                                to_remove.push(tx_id);
                            } else {
                                to_broadcast.push((tx_id, tx.index));
                            }
                        }
                    }
                    _ => {}
                }
            }

            pending.push((
                (env_id, cannon_id),
                PendingTransactions {
                    to_execute: sorted_by_index(to_execute),
                    to_broadcast: sorted_by_index(to_broadcast),
                    to_remove,
                    to_confirm: sorted_by_index(to_confirm),
                },
            ));
        }
    }

    pending
}

/// Sort a vec of values by their index
fn sorted_by_index<T>(mut vec: Vec<(T, u64)>) -> Vec<T> {
    vec.sort_by_key(|(_, index)| *index);
    vec.into_iter().map(|(first, _)| first).collect()
}
