use lasso::Spur;
use serde::{Deserialize, Serialize};

use crate::{node_targets::NodeTargets, INTERN};

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

impl Default for ComputeTarget {
    fn default() -> Self {
        ComputeTarget::Agent { labels: None }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_from: Option<NodeTargets>,
}
