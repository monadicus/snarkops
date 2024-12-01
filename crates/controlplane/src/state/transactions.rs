use std::{sync::Arc, time::Duration};

use chrono::{TimeDelta, Utc};
use futures_util::future;
use snops_common::state::{CannonId, EnvId};
use tokio::time::timeout;
use tracing::{info, trace};

use super::GlobalState;
use crate::{
    cannon::{status::TransactionSendState, tracker::TransactionTracker},
    events::{EventHelpers, TransactionEvent},
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
                        .send(tx_id.clone())
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
                        let (tx_id, hash) = if let Some(hash) = state.env_network_cache.get(&env_id).and_then(|cache| cache.find_transaction(&tx_id).cloned()) {
                            trace!("cannon {env_id}.{cannon_id} confirmed transaction {tx_id} (cache hit)");
                            (tx_id, hash.to_string())
                        }

                        // check if the transaction not is in the cache, then check the peers
                        else if let Some(target) = cannon_target {
                            match timeout(Duration::from_secs(1),
                            state.snarkos_get::<Option<String>>(env_id, format!("/find/blockHash/{tx_id}"), target)).await {
                                Ok(Ok(Some(hash))) => {
                                    trace!("cannon {env_id}.{cannon_id} confirmed transaction {tx_id} (get request)");
                                    (tx_id, hash)
                                }
                                // the transaction is not in the cache
                                _ => return None,
                            }
                        } else {
                            return None;
                        };

                        // Emit a confirmed event
                        TransactionEvent::Confirmed { hash }
                            .with_cannon(cannon_id)
                            .with_env_id(env_id)
                            .with_transaction(Arc::clone(&tx_id)).emit(&state);

                        Some(tx_id)
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
    to_execute: Vec<Arc<String>>,
    to_broadcast: Vec<Arc<String>>,
    to_remove: Vec<Arc<String>>,
    to_confirm: Vec<(Arc<String>, Option<u32>)>,
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
                let key = (env_id, cannon_id, Arc::clone(&tx_id));
                let attempts = TransactionTracker::get_attempts(state, &key);

                let ev = TransactionEvent::Executing
                    .with_cannon(cannon_id)
                    .with_env_id(env_id)
                    .with_transaction(Arc::clone(&tx_id));

                match tx.status {
                    // any authorized transaction that is not started should be queued
                    TransactionSendState::Authorized => {
                        if cannon.sink.authorize_attempts.is_some_and(|a| attempts > a) {
                            info!("cannon {env_id}.{cannon_id} removed auth {tx_id} (too many attempts)");
                            to_remove.push(tx_id);
                            ev.replace_kind(TransactionEvent::ExecuteExceeded { attempts })
                                .emit(state);
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
                            ev.replace_kind(TransactionEvent::ExecuteExceeded { attempts })
                                .emit(state);
                            to_remove.push(tx_id);
                        } else {
                            to_execute.push((tx_id, tx.index));
                        }
                    }
                    // any unbroadcasted transaction that is not started should be queued
                    TransactionSendState::Unsent => {
                        if cannon.sink.broadcast_attempts.is_some_and(|a| attempts > a) {
                            info!("cannon {env_id}.{cannon_id} removed broadcast {tx_id} (too many attempts)");
                            ev.replace_kind(TransactionEvent::BroadcastExceeded { attempts })
                                .emit(state);
                            to_remove.push(tx_id);
                        } else {
                            to_broadcast.push((tx_id, tx.index));
                        }
                    }
                    // any expired broadcast should be queued
                    // any broadcast that has a different height than the latest height
                    // should be confirmed
                    TransactionSendState::Broadcasted(height, broadcast_time) => {
                        let height_changed = match (height, latest_height) {
                            // latest height is higher
                            (Some(height), Some(latest_height)) => latest_height > height,
                            // latest height is unknown
                            (None, None) => false,
                            // heights have changed
                            _ => true,
                        };

                        if !height_changed {
                            continue;
                        }

                        // When the block height changes, queue a confirm.
                        // This feature is only available for sinks with a target (should be
                        // unreachable either way)
                        if cannon.sink.target.is_some() {
                            to_confirm.push(((tx_id.clone(), height), tx.index));
                        }

                        // Queue a re-broadcast if the broadcast has timed out.
                        // Also requires the block height to change.
                        if now - broadcast_time
                            > TimeDelta::seconds(cannon.sink.broadcast_timeout as i64)
                        {
                            if cannon.sink.broadcast_attempts.is_some_and(|a| attempts > a) {
                                info!("cannon {env_id}.{cannon_id} removed broadcast {tx_id} (too many attempts)");
                                ev.replace_kind(TransactionEvent::BroadcastExceeded { attempts })
                                    .emit(state);
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
