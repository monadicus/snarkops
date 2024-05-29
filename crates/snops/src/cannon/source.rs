use serde::{Deserialize, Serialize};
use serde_json::json;
use snops_common::{
    lasso::Spur,
    state::{NetworkId, NodeKey},
    INTERN,
};

use super::{
    error::{CannonError, SourceError},
    net::get_available_port,
};
use crate::{
    env::{set::find_compute_agent, Environment},
    schema::NodeTargets,
    state::GlobalState,
};

/// Represents an instance of a local query service.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalService {
    // TODO debate this
    /// An optional node to sync blocks from...
    /// necessary for private tx mode in realtime mode as this will have to
    /// sync from a node that has a valid ledger
    ///
    /// When present, the cannon will update the ledger service from this node
    /// if the node is out of sync, it will corrupt the ledger...
    ///
    /// requires cannon to have an associated env_id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_from: Option<NodeKey>,
}

impl LocalService {
    // TODO: cache this when sync_from is false
    /// Fetch the state root from the local query service
    /// (non-cached)
    pub async fn get_state_root(
        &self,
        network: NetworkId,
        port: u16,
    ) -> Result<String, CannonError> {
        let url = format!("http://127.0.0.1:{port}/{network}/latest/stateRoot");
        let response = reqwest::get(&url)
            .await
            .map_err(|e| SourceError::FailedToGetStateRoot(url, e))?;
        Ok(response
            .json()
            .await
            .map_err(SourceError::StateRootInvalidJson)?)
    }
}

/// Used to determine the redirection for the following paths:
/// /cannon/<id>/<network>/latest/stateRoot
/// /cannon/<id>/<network>/transaction/broadcast
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", untagged)]
pub enum QueryTarget {
    /// Target a specific node (probably over rpc instead of reqwest lol...)
    ///
    /// Requires cannon to have an associated env_id
    Node(NodeTargets),
    /// Use the local ledger query service
    Local(LocalService),
}

impl Default for QueryTarget {
    fn default() -> Self {
        QueryTarget::Local(LocalService { sync_from: None })
    }
}

fn deser_labels<'de, D>(deser: D) -> Result<Option<Vec<Spur>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<String>>::deserialize(deser)?.map(|s| {
        s.into_iter()
            .map(|s| INTERN.get_or_intern(s))
            .collect::<Vec<Spur>>()
    }))
}

fn ser_labels<S>(labels: &Option<Vec<Spur>>, ser: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match labels {
        Some(labels) => {
            let labels = labels
                .iter()
                .map(|s| INTERN.resolve(s))
                .collect::<Vec<&str>>();
            serde::Serialize::serialize(&labels, ser)
        }
        None => serde::Serialize::serialize(&None::<String>, ser),
    }
}

/// Which service is providing the compute power for executing transactions
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", untagged)]
pub enum ComputeTarget {
    /// Use the agent pool to generate executions
    Agent {
        #[serde(
            default,
            deserialize_with = "deser_labels",
            serialize_with = "ser_labels",
            skip_serializing_if = "Option::is_none"
        )]
        labels: Option<Vec<Spur>>,
    },
    /// Use demox' API to generate executions
    #[serde(rename_all = "kebab-case")]
    Demox { demox_api: String },
}

impl Default for ComputeTarget {
    fn default() -> Self {
        ComputeTarget::Agent { labels: None }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CreditsTxMode {
    BondPublic,
    UnbondPublic,
    TransferPublic,
    TransferPublicToPrivate,
    // cannot run these in aot mode
    TransferPrivate,
    TransferPrivateToPublic,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TxMode {
    Credits(CreditsTxMode),
    // TODO: Program(program, func, input types??)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TxSource {
    /// Receive authorizations from a persistent path
    /// /api/v1/env/:env_id/cannons/:id/auth
    #[serde(default)]
    pub query: QueryTarget,
    #[serde(default)]
    pub compute: ComputeTarget,
}

impl TxSource {
    /// Get an available port for the query service if applicable
    pub fn get_query_port(&self) -> Result<Option<u16>, CannonError> {
        if !matches!(self.query, QueryTarget::Local(_)) {
            return Ok(None);
        }
        Ok(Some(
            get_available_port().ok_or(SourceError::TxSourceUnavailablePort)?,
        ))
    }
}

impl ComputeTarget {
    pub async fn execute(
        &self,
        state: &GlobalState,
        env: &Environment,
        query_path: &str,
        auth: &serde_json::Value,
        fee_auth: Option<&serde_json::Value>,
    ) -> Result<(), CannonError> {
        match self {
            ComputeTarget::Agent { labels } => {
                // find a client, mark it as busy
                let (client, _busy) =
                    find_compute_agent(state, &labels.clone().unwrap_or_default())
                        .ok_or(SourceError::NoAvailableAgents("authorization"))?;

                // execute the authorization
                client
                    .execute_authorization(
                        env.id,
                        env.network,
                        query_path.to_owned(),
                        serde_json::to_string(&auth)
                            .map_err(|e| SourceError::Json("authorize tx", e))?,
                        fee_auth
                            .map(|f| {
                                serde_json::to_string(&f)
                                    .map_err(|e| SourceError::Json("authorize fee", e))
                            })
                            .transpose()?,
                    )
                    .await?;

                Ok(())
            }
            ComputeTarget::Demox { demox_api: url } => {
                let _body = json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "generateTransaction",
                    "params": {
                        "authorization": serde_json::to_string(&auth).map_err(|e| SourceError::Json("authorize tx", e))?,
                        "fee": serde_json::to_string(&fee_auth).map_err(|e| SourceError::Json("authorize fee", e))?,
                        "url": query_path,
                        "broadcast": true,
                    }
                });

                todo!("post on {url}")
            }
        }
    }
}

// I use this to generate example yaml...
/* #[cfg(test)]
mod test {
    use super::*;
    use crate::{
        cannon::source::{ComputeTarget, CreditsTxMode, LocalService, TxMode},
        schema::nodes::KeySource,
    };
    use std::str::FromStr;

    #[test]
    fn what_does_it_look_like() {
        println!(
            "{}",
            serde_yaml::to_string(&TxSource::Playback {
                file_name: "test".to_string(),
            })
            .unwrap()
        );
        println!(
            "{}",
            serde_yaml::to_string(&TxSource::RealTime {
                query: QueryTarget::Local(LocalService { sync_from: None }),
                compute: ComputeTarget::Agent { labels: None },
                tx_modes: [TxMode::Credits(CreditsTxMode::TransferPublic)]
                    .into_iter()
                    .collect(),
                private_keys: vec![KeySource::from_str("committee.$").unwrap()],
                addresses: vec![KeySource::from_str("committee.$").unwrap()],
            })
            .unwrap()
        );
    }
}
 */
