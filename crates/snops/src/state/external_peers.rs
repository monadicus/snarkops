use std::{collections::HashMap, net::SocketAddr, ops::Deref, sync::Arc, time::Duration};

use chrono::Utc;
use futures_util::future;
use serde_json::Value;
use snops_common::state::{EnvId, LatestBlockInfo, NetworkId, NodeKey};
use tokio::time::timeout;

use super::{snarkos_request, AgentClient, GlobalState};
use crate::{
    env::{
        cache::{ABlockHash, ATransactionId},
        EnvNodeState, EnvPeer,
    },
    schema::nodes::ExternalNode,
};

type ExtPeerPair = (NodeKey, SocketAddr);
type PendingBlockRequests = HashMap<(EnvId, NetworkId, ABlockHash), Vec<ExtPeerPair>>;

/// Hit all the external peers to update their latest block infos.
///
/// If an external peer has a new block info and transaction list, update the
/// cache with the new data.
pub async fn external_block_info_task(state: Arc<GlobalState>) {
    loop {
        // Get applicable external peers. This is unfiltered as all block info can be
        // expected to be out of date before the next time this loop is run.
        let external_rest_peers = get_all_external_peers(&state);

        // fetch the latest block hashes for EVERY external peer across EVERY
        // environment
        let peers_with_block_hashes = future::join_all(external_rest_peers.into_iter().map(
            |((env, network), peers)| async move {
                let peers = future::join_all(peers.into_iter().map(|(key, addr)| async move {
                    timeout(
                        // short timeout for block hash requests as not much is being
                        // serialized on snarkOS side
                        Duration::from_secs(1),
                        get_block_hash_for_peer(network, addr),
                    )
                    .await
                    .ok()
                    .and_then(|hash| hash.map(|h| (key, addr, h)))
                }))
                .await;
                ((env, network), peers)
            },
        ))
        .await;

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
            for (key, addr, hash) in peers_and_hashes.into_iter().flatten() {
                // update the peer's block info if it is different than the peer's current info
                cache.update_peer_info_for_hash(&key, &hash);

                // prevent re-requesting the list of transactions for a block that
                // is already cached
                if cache.block_to_transaction.contains_key(&hash) {
                    continue;
                }

                // update the list of blocks that need to be requested
                blocks_pending_request
                    .entry((env, network, hash.clone()))
                    .or_default()
                    .push((key, addr));
            }
        }

        // fetch the missing block info from agents if possible (fallback on external
        // peers), then update the cache with the peer data
        let block_fetch_tasks =
            blocks_pending_request
                .into_iter()
                .map(|((env, network, hash), peers)| {
                    async move {
                        // TODO: check agents that may have this height/hash available then make the
                        // request on those agents let applicable_agents =

                        // Some((env, network, hash, info))
                        todo!()
                    }
                });

        // wait 10 seconds between checks
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

// todo: check if an external peer's /block/hash/latest is already cached before
// running this

/// Obtain a peer's latest block hash
async fn get_block_hash_for_peer(network: NetworkId, addr: SocketAddr) -> Option<Arc<str>> {
    // make a request to the external peer for the latest block hash
    // TODO: there is no api to get the block height for a block hash, and no API
    // for getting the block hash from a height
    let res = snarkos_request::get_on_addr::<Value>(network, "/block/hash/latest", addr)
        .await
        .ok()?;
    Some(res.as_str()?.into())
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

async fn update_info_for_peer(
    state: &GlobalState,
    node: NodeKey,
    env_id: EnvId,
    info: LatestBlockInfo,
) {
    let mut cache = state.env_network_cache.entry(env_id).or_default();
    // update the latest block info
    cache.update_latest_info(&info);
    // update info for the specific peer
    cache.update_peer_info(node, info);
}

// Compute a list of all external peers that have rest addresses
fn get_all_external_peers(state: &GlobalState) -> Vec<((EnvId, NetworkId), Vec<ExtPeerPair>)> {
    state
        .envs
        .iter()
        .map(|e| {
            (
                // environment meta required for requests and cache updates
                (*e.key(), e.network),
                // iterate the environment's nodes
                e.node_states
                    .iter()
                    .filter_map(|n| match n.value() {
                        // filter by external with rest addresses
                        EnvNodeState::External(ExternalNode {
                            rest: Some(addr), ..
                        }) => Some((n.key().clone(), *addr)),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            )
        })
        // collect here to avoid holding a dashmap read lock for too long
        .collect::<Vec<_>>()
}
