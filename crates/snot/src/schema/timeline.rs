use std::{fmt, time::Duration};

use indexmap::IndexMap;
use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer,
};
use snot_common::state::NodeKey;

use super::NodeTargets;

/// A document describing a test's event timeline.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub timeline: Vec<TimelineEvent>,
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
pub struct Actions(Vec<ActionInstance>);

#[derive(Debug, Clone)]
pub struct ActionInstance {
    pub action: Action,
    pub awaited: bool,
}

#[derive(Debug, Clone)]
pub enum Action {
    /// Update the given nodes to an online state
    Online(NodeTarget),
    /// Update the given nodes to an offline state
    Offline(NodeTarget),
    /// Fire transactions from a source file at a target node
    Cannon(Vec<SpawnCannon>),
    /// Set the height of some nodes' ledgers
    Height(IndexMap<String, u64>),
}

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
                    "height" => Action::Height(map.next_value()?),

                    _ => return Err(A::Error::custom(format!("unsupported action {key}"))),
                },
            });
        }

        Ok(Actions(buf))
    }
}

impl<'de> Deserialize<'de> for Actions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(ActionsVisitor)
    }
}

/// A target for an event.
#[derive(Debug, Clone)]
pub enum NodeTarget {
    Some(Vec<String>),
    All,
}

struct NodeTargetVisitor;

impl<'de> Visitor<'de> for NodeTargetVisitor {
    type Value = NodeTarget;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a list of node IDs or the string \"all\"")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match v {
            "all" => Ok(NodeTarget::All),
            _ => Err(E::custom("string must be \"all\"")),
        }
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut buf = Vec::new();
        while let Some(id) = seq.next_element()? {
            buf.push(id);
        }
        Ok(NodeTarget::Some(buf))
    }
}

impl<'de> Deserialize<'de> for NodeTarget {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(NodeTargetVisitor)
    }
}

#[derive(Debug, Clone)]
pub enum EventDuration {
    Time(Duration),
    Blocks(u64),
}

struct EventDurationVisitor;

impl<'de> Visitor<'de> for EventDurationVisitor {
    type Value = EventDuration;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string duration or an integer number of blocks to be produced")
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

impl<'de> Deserialize<'de> for EventDuration {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(EventDurationVisitor)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct SpawnCannon {
    pub name: String,
    pub count: u64,
    /// overwrite the query's source node
    pub query: Option<NodeKey>,
    /// overwrite the cannon sink target
    pub target: Option<NodeTargets>,
}
