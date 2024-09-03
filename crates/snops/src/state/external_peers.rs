use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use chrono::{TimeDelta, Utc};
use futures_util::future;
use serde_json::Value;
use snops_common::state::{EnvId, LatestBlockInfo, NetworkId, NodeKey};
use tokio::{sync::mpsc, time::timeout};

use super::{snarkos_request, AgentClient, GlobalState};
use crate::{
    env::{
        cache::{ABlockHash, ATransactionId, MAX_BLOCK_RANGE},
        EnvNodeState, EnvPeer,
    },
    schema::nodes::ExternalNode,
};

type ExtPeerPair = (NodeKey, SocketAddr);
type PendingBlockRequests =
    HashMap<(EnvId, NetworkId), HashMap<ABlockHash, (u32, Vec<ExtPeerPair>)>>;

/// Hit all the external peers to update their latest block infos.
///
/// If an external peer has a new block info and transaction list, update the
/// cache with the new data.
pub async fn block_info_task(state: Arc<GlobalState>) {
    loop {
        // Get applicable external peers. This is unfiltered as all block info can be
        // expected to be out of date before the next time this loop is run.
        let external_rest_peers = get_all_external_peers(&state);

        // channel to measure the success of peer requests
        let (req_ok_tx, mut req_ok_rx) = mpsc::unbounded_channel();

        // fetch the latest block hashes for EVERY external peer across EVERY
        // environment
        let peers_with_block_hashes = future::join_all(external_rest_peers.into_iter().map(
            |((env, network), peers)| {
                let req_ok_tx = req_ok_tx.clone();
                async move {
                    let peers = future::join_all(peers.into_iter().map(|(key, addr)| {
                        let req_ok_tx = req_ok_tx.clone();
                        async move {
                            let res = timeout(
                                // short timeout for block hash requests as not much is being
                                // serialized on snarkOS side
                                Duration::from_secs(1),
                                get_block_hash_for_peer(network, addr),
                            )
                            .await
                            .ok()
                            .and_then(|hash| hash.map(|h| (key.clone(), addr, h)));
                            // mark down a successful request
                            req_ok_tx.send((env, key, res.is_some())).ok();
                            res
                        }
                    }))
                    .await;
                    ((env, network), peers)
                }
            },
        ))
        .await;

        let now = Utc::now();

        // map of block hashes and environments to peers that can provide them
        // TODO: fetch this from an AOT peer instead if possible
        let mut blocks_pending_request: PendingBlockRequests = HashMap::new();

        // Go through each env and peer info
        for ((env, network), peers_and_hashes) in peers_with_block_hashes {
            // If there is no cache we skip
            let Some(mut cache) = state.env_network_cache.get_mut(&env) else {
                continue;
            };

            // Go through each peer for an env if they were responsive with the block hash
            // request (flatten)
            for (key, addr, (hash, height)) in peers_and_hashes.into_iter().flatten() {
                // update the peer's block info if it is different than the peer's current info
                cache.update_peer_info_for_hash(&key, &hash);

                // prevent re-requesting the list of transactions for a block that
                // is already cached
                if cache.block_to_transaction.contains_key(&hash) {
                    continue;
                }

                // prevent making a request on a peer that is probably syncing (way out of date
                // height)
                if cache.latest.as_ref().is_some_and(|i|
                        // peer's height outside the max block range
                        i.height.saturating_sub(MAX_BLOCK_RANGE) >= height
                        // and the block range is recent
                        && (now - i.update_time) < TimeDelta::seconds(60))
                {
                    continue;
                }

                use std::collections::hash_map::Entry::*;
                // update the list of blocks that need to be requested
                match blocks_pending_request
                    .entry((env, network))
                    .or_default()
                    .entry(hash)
                {
                    // append this peer to the list of peers that can provide
                    // use the min height because of a slim chance that the latest block changed
                    // in the time between the height and hash requests.
                    Occupied(e) => {
                        let e = e.into_mut();
                        e.0 = e.0.min(height);
                        e.1.push((key, addr));
                    }
                    // insert this height and peer into the list of peers that can provide
                    Vacant(e) => {
                        e.insert((height, vec![(key, addr)]));
                    }
                }
            }
        }

        // fetch the missing block info from agents if possible (fallback on external
        // peers), then update the cache with the peer data
        let block_request_tasks = future::join_all(blocks_pending_request.into_iter().map(
            |((env, network), requests)| {
                // highest height of all requests
                let max_height = requests.values().map(|(height, _)| *height).max().unwrap();
                // list of agents that could fulfil this request (rather than making slow rest &
                // deserialize requests)
                let agents = Arc::new(online_agents_above_height(&state, env, max_height));
                let req_ok_tx = req_ok_tx.clone();

                async move {
                    (
                        env,
                        future::join_all(requests.into_iter().map(|(hash, peers)| {
                            let req_ok_tx = req_ok_tx.clone();
                            let agents = agents.clone();

                            // peer keys to update (or request)
                            let keys = peers
                                .1
                                .iter()
                                .map(|(key, _)| key.clone())
                                .collect::<Vec<_>>();

                            async move {
                                // attempt to use agents to get the block
                                if let Some(res) =
                                    get_block_from_agents(&agents, Arc::clone(&hash)).await
                                {
                                    return Some((res, keys));
                                }

                                // if agents failed, fallback on external peers
                                let mut failures = 0u8;
                                for (key, addr) in peers.1 {
                                    if let Some(res) = get_block_info_for_peer(network, addr).await
                                    {
                                        let _ = req_ok_tx.send((env, key, true)).ok();
                                        return Some((res, keys));
                                    }
                                    let _ = req_ok_tx.send((env, key, false)).ok();
                                    failures += 1;
                                    if failures >= MAX_BLOCK_REQUEST_FAILURES {
                                        break;
                                    }
                                }

                                None
                            }
                        }))
                        .await,
                    )
                }
            },
        ))
        .await;

        // update the cache with the request results
        while let Ok((env, key, success)) = req_ok_rx.try_recv() {
            let Some(mut cache) = state.env_network_cache.get_mut(&env) else {
                continue;
            };

            cache.update_peer_req(&key, success);
        }

        // update the chache with the block info and transaction ids
        // from the block requests
        for (env, responses) in block_request_tasks {
            let Some(mut cache) = state.env_network_cache.get_mut(&env) else {
                continue;
            };

            // update the cache with the block info and transaction ids
            // then update each peer's info
            for ((info, txs), keys) in responses.into_iter().flatten() {
                cache.add_block(info.clone(), txs);
                for key in keys {
                    cache.update_latest_info(&info);
                    cache.update_peer_info(key, info.clone());
                }
            }
        }

        // wait 10 seconds between checks, including the time it took to process
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}

/// Get all online agents above a certain height in an environment
fn online_agents_above_height(state: &GlobalState, env: EnvId, height: u32) -> Vec<AgentClient> {
    let Some(env) = state.get_env(env) else {
        return Vec::new();
    };

    env.node_peers
        .iter()
        .filter_map(|(_, peer)| {
            // ensure peer is internal
            let EnvPeer::Internal(agent_id) = peer else {
                return None;
            };
            let agent = state.pool.get(agent_id)?;
            // ensure peer height is above or equal the requested height
            if agent.status.block_info.as_ref()?.height < height {
                return None;
            }
            // ensure the agent is online
            agent.client_owned()
        })
        .collect()
}

/// Obtain a peer's latest block hash and height
/// We do not assume the hash and height are related, and they are used for
/// separate purposes.
async fn get_block_hash_for_peer(network: NetworkId, addr: SocketAddr) -> Option<(Arc<str>, u32)> {
    // make a request to the external peer for the latest block hash
    let hash_res = snarkos_request::get_on_addr::<Value>(network, "/block/hash/latest", addr)
        .await
        .ok()?;
    let height_res = snarkos_request::get_on_addr::<Value>(network, "/block/hash/latest", addr)
        .await
        .ok()?;

    Some((hash_res.as_str()?.into(), height_res.as_u64()? as u32))
}

const MAX_BLOCK_REQUEST_FAILURES: u8 = 3;

/// Obtain a block from a list of agents, permits up to 3 failures
async fn get_block_from_agents(
    agents: &Vec<AgentClient>,
    hash: ABlockHash,
) -> Option<(LatestBlockInfo, Vec<ATransactionId>)> {
    let mut failures = 0u8;
    for agent in agents {
        if let Ok(Some(block)) = agent.get_snarkos_block_lite(hash.to_string()).await {
            return Some(block.split());
        }
        failures += 1;
        if failures >= MAX_BLOCK_REQUEST_FAILURES {
            break;
        }
    }
    None
}

/// Obtain a peer's block info and transaction ids
async fn get_block_info_for_peer(
    network: NetworkId,
    addr: SocketAddr,
) -> Option<(LatestBlockInfo, Vec<ATransactionId>)> {
    // make a request to the external peer for the latest block
    // TODO: make this a RawValue to prevent unnecessarily parsing the response
    let Ok(block_raw) = snarkos_request::get_on_addr::<Value>(network, "/block/latest", addr).await
    else {
        tracing::trace!("failed to get latest block for peer: {addr:?}");
        return None;
    };
    let Some(block_hash) = block_raw.get("block_hash").and_then(|s| s.as_str()) else {
        tracing::trace!("block request for peer is missing block hash: {addr:?}");
        return None;
    };
    let Some(previous_hash) = block_raw.get("previous_hash").and_then(|s| s.as_str()) else {
        tracing::trace!("block request for peer is missing previous hash: {addr:?}");
        return None;
    };
    let Some(header) = block_raw.get("header").and_then(|h| h.get("metadata")) else {
        tracing::trace!("block request for peer is missing header metadata: {addr:?}");
        return None;
    };
    let Some(height) = header
        .get("height")
        .and_then(|h| h.as_u64().map(|h| h as u32))
    else {
        tracing::trace!("block request for peer is missing block height: {addr:?}");
        return None;
    };
    let Some(block_timestamp) = header.get("timestamp").and_then(|t| t.as_i64()) else {
        tracing::trace!("block request for peer is missing block timestamp: {addr:?}");
        return None;
    };

    let Some(txs_raw) = block_raw.get("transactions").and_then(|t| t.as_array()) else {
        tracing::trace!("block request for peer is missing transactions: {addr:?}");
        return None;
    };

    // fetch the state root (because it's missing from the block)
    let route = format!("/stateRoot/{height}");
    let Ok(state_root) = snarkos_request::get_on_addr::<String>(network, &route, addr).await else {
        tracing::trace!("failed to get state root for peer: {addr:?}");
        return None;
    };

    // assemble transaction ids from valid json value
    let mut txs = Vec::with_capacity(txs_raw.len());
    for tx in txs_raw {
        let Some(tx_id) = tx
            .get("transaction")
            .and_then(|tx| tx.get("id").and_then(|id| id.as_str()))
        else {
            tracing::trace!("transaction is missing tx_id: {tx:?}");
            continue;
        };
        txs.push(Arc::from(tx_id));
    }

    Some((
        LatestBlockInfo {
            height,
            state_root,
            block_hash: block_hash.to_owned(),
            block_timestamp,
            previous_hash: previous_hash.to_owned(),
            update_time: Utc::now(),
        },
        txs,
    ))
}

// Compute a list of all external peers that have rest addresses
fn get_all_external_peers(state: &GlobalState) -> Vec<((EnvId, NetworkId), Vec<ExtPeerPair>)> {
    state
        .envs
        .iter()
        .map(|e| {
            let Some(cache) = state.env_network_cache.get(e.key()) else {
                return ((*e.key(), e.network), Vec::new());
            };

            (
                // environment meta required for requests and cache updates
                (*e.key(), e.network),
                // iterate the environment's nodes
                e.node_states
                    .iter()
                    .filter_map(|n| {
                        // skip unresponsive peers
                        if cache.is_peer_penalized(n.key()) {
                            return None;
                        }

                        match n.value() {
                            // filter by external with rest addresses
                            EnvNodeState::External(ExternalNode {
                                rest: Some(addr), ..
                            }) => Some((n.key().clone(), *addr)),
                            _ => None,
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        })
        // collect here to avoid holding a dashmap read lock for too long
        .collect::<Vec<_>>()
}
