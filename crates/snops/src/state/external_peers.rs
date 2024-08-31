use std::{net::SocketAddr, sync::Arc};

use chrono::Utc;
use serde_json::{value::RawValue, Value};
use snops_common::state::{EnvId, LatestBlockInfo, NetworkId, NodeKey, NodeState};

use super::{snarkos_request, GlobalState};
use crate::{
    env::{EnvNodeState, EnvPeer},
    schema::nodes::ExternalNode,
};

async fn get_block_info_for_peer(network: NetworkId, addr: SocketAddr) -> Option<LatestBlockInfo> {
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

    // fetch the state root (because it's missing from the block)
    let route = format!("/stateRoot/{height}");
    let Ok(state_root) = snarkos_request::get_on_addr::<String>(network, &route, addr).await else {
        tracing::trace!("failed to get state root for peer: {addr:?}");
        return None;
    };

    Some(LatestBlockInfo {
        height,
        state_root,
        block_hash: block_hash.to_owned(),
        block_timestamp,
        update_time: Utc::now(),
    })
}

async fn update_info_for_peer(
    state: &GlobalState,
    node: NodeKey,
    env_id: EnvId,
    info: LatestBlockInfo,
) {
    // todo: update the external peer's block info somewhere
    state.update_env_block_info(env_id, &info)
}

pub async fn external_block_info_task(state: Arc<GlobalState>) {
    loop {
        // obtain a list of all external peers that have rest addresses
        let external_rest_peers = state
            .envs
            .iter()
            .flat_map(|e| {
                // iterate the environment's nodes
                e.node_states
                    .iter()
                    .filter_map(|n| match n.value() {
                        // filter by external with rest addresses
                        EnvNodeState::External(ExternalNode {
                            rest: Some(addr), ..
                        }) => Some((*e.key(), e.network, n.key().clone(), *addr)),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            // collect here to avoid holding a dashmap read lock for too long
            .collect::<Vec<_>>();

        // wait 10 seconds between checks
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}
