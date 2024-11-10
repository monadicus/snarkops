use std::net::SocketAddr;

use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use snops_common::{rpc::error::SnarkosRequestError, state::NetworkId};

use super::REST_CLIENT;
use crate::env::error::EnvRequestError;

/// I would rather reparse a string than use unsafe/dyn any here
/// because we would be making a request anyway and it's not a big deal.
pub fn reparse_json<T: DeserializeOwned>(v: impl Serialize) -> Result<T, serde_json::Error> {
    serde_json::from_value(json!(&v))
}

/// This is the same as `json_generics_bodge` but it returns a `EnvRequestError`
pub fn reparse_json_env<T: DeserializeOwned>(v: impl Serialize) -> Result<T, EnvRequestError> {
    serde_json::from_value(json!(&v)).map_err(|e| {
        EnvRequestError::AgentRequestError(SnarkosRequestError::JsonParseError(e.to_string()))
    })
}

/// A list of routes that are prefixes of the routes covered by the
/// `LatestBlockInfo`
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RoutePrefix {
    StateRoot,
    BlockHeight,
    BlockHash,
}

/// Helper function to check if a route is a prefix of a route covered by the
/// `LatestBlockInfo`
pub fn route_prefix_check(route: &str) -> Option<RoutePrefix> {
    match route {
        "/latest/stateRoot" | "/stateRoot/latest" => Some(RoutePrefix::StateRoot),
        "/latest/height" | "/block/height/latest" => Some(RoutePrefix::BlockHeight),
        "/latest/hash" | "/block/hash/latest" => Some(RoutePrefix::BlockHash),
        _ => None,
    }
}

pub async fn get_on_addr<T: DeserializeOwned>(
    network: NetworkId,
    route: &str,
    addr: SocketAddr,
) -> Result<T, SnarkosRequestError> {
    let url = format!("http://{addr}/{network}{route}");
    let request = REST_CLIENT.get(&url).send();

    // make the request with a 1 second timeout, then parse the response as json
    tokio::time::timeout(std::time::Duration::from_secs(5), request)
        .await
        .map_err(|_| SnarkosRequestError::TimedOut)?
        .map_err(|e| SnarkosRequestError::RequestError(e.to_string()))?
        .json::<T>()
        .await
        .map_err(|e| SnarkosRequestError::JsonParseError(e.to_string()))
}
