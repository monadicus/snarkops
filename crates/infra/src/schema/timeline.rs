use std::{fmt, path::PathBuf, time::Duration};

use indexmap::IndexMap;
use serde::{de::Visitor, Deserialize, Serialize};

/// A document describing a test's event timeline.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Document {
    pub timeline: Vec<TimelineEvent>,
}

/// An event in the test timeline.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TimelineEvent {
    pub duration: Option<EventDuration>,
    pub online: Option<NodeTarget>,
    pub offline: Option<NodeTarget>,

    #[serde(rename = "online.await")]
    pub online_await: Option<NodeTarget>,

    #[serde(rename = "offline.await")]
    pub offline_await: Option<NodeTarget>,

    // TODO: connections, not really sure what this is about
    pub height: Option<IndexMap<String, u64>>,
    pub cannons: Option<Vec<TxCannon>>,
}

/// A target for an event.
#[derive(Serialize, Debug, Clone)]
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
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(NodeTargetVisitor)
    }
}

#[derive(Serialize, Debug, Clone)]
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
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(EventDurationVisitor)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxCannon {
    pub target: String,
    pub source: PathBuf,
    pub total: u64,
    pub tps: u32,
}
