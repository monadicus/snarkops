use std::{collections::HashMap, fmt, time::Duration};

use indexmap::IndexMap;
use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer, Serialize,
};
use snops_common::state::{CannonId, DocHeightRequest, InternedId, NodeKey};

use super::{outcomes::OutcomeExpectation, NodeTargets};

pub type OutcomeMetrics = HashMap<String, OutcomeExpectation>;

/// A document describing a test's event timeline.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub name: InternedId,
    pub description: Option<String>,
    #[serde(default)]
    pub timeline: Vec<TimelineEvent>,
    #[serde(default)]
    pub outcomes: OutcomeMetrics,
}

/// An event in the test timeline.
#[derive(Deserialize, Debug, Clone)]
pub struct TimelineEvent {
    /// The event will run for at least the given duration
    pub duration: Option<EventDuration>,

    /// An awaited action will error if it does not occur within the given
    /// duration
    pub timeout: Option<EventDuration>,

    #[serde(flatten)]
    pub actions: Actions,
}

#[derive(Debug, Clone)]
pub struct Actions(pub Vec<ActionInstance>);

#[derive(Debug, Clone)]
pub struct ActionInstance {
    pub action: Action,
    pub awaited: bool,
}

#[derive(Debug, Clone)]
pub enum Action {
    /// Update the given nodes to an online state
    Online(NodeTargets),
    /// Update the given nodes to an offline state
    Offline(NodeTargets),
    /// Fire transactions from a source file at a target node
    Cannon(Vec<SpawnCannon>),
    /// Set the height of some nodes' ledgers
    Config(IndexMap<NodeTargets, Reconfig>),
}

impl<'de> Deserialize<'de> for Actions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ActionsVisitor;

        impl<'de> Visitor<'de> for ActionsVisitor {
            type Value = Actions;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("possibly awaited action map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut buf = vec![];

                while let Some(key) = map.next_key::<&str>()? {
                    // determine if this action is being awaited
                    let (key, awaited) = match key {
                        key if key.ends_with(".await") => (key.split_at(key.len() - 6).0, true),
                        _ => (key, false),
                    };

                    buf.push(ActionInstance {
                        awaited,
                        action: match key {
                            "online" => Action::Online(map.next_value()?),
                            "offline" => Action::Offline(map.next_value()?),
                            "cannon" => Action::Cannon(map.next_value()?),
                            "config" => Action::Config(map.next_value()?),

                            _ => return Err(A::Error::custom(format!("unsupported action {key}"))),
                        },
                    });
                }

                Ok(Actions(buf))
            }
        }

        deserializer.deserialize_map(ActionsVisitor)
    }
}

#[derive(Debug, Clone)]
pub enum EventDuration {
    Time(Duration),
    Blocks(u64),
}

impl<'de> Deserialize<'de> for EventDuration {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EventDurationVisitor;

        impl<'de> Visitor<'de> for EventDurationVisitor {
            type Value = EventDuration;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter
                    .write_str("a string duration or an integer number of blocks to be produced")
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(EventDuration::Blocks(v))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(EventDuration::Time(
                    duration_str::parse(v).map_err(E::custom)?,
                ))
            }
        }

        deserializer.deserialize_any(EventDurationVisitor)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct SpawnCannon {
    pub name: CannonId,
    #[serde(default)]
    pub count: Option<usize>,
    /// overwrite the query's source node
    #[serde(default)]
    pub query: Option<NodeKey>,
    /// overwrite the cannon sink target
    #[serde(default)]
    pub target: Option<NodeTargets>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reconfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<DocHeightRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peers: Option<NodeTargets>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validators: Option<NodeTargets>,
}
