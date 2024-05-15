use std::{
    collections::HashMap,
    fmt,
    io::{Read, Write},
    time::Duration,
};

use indexmap::IndexMap;
use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer, Serialize,
};
use snops_common::{
    format::{
        read_dataformat, write_dataformat, DataFormat, DataFormatReader, DataHeaderOf,
        DataReadError, DataWriteError,
    },
    state::{CannonId, DocHeightRequest, InternedId, NodeKey},
};

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

impl DataFormat for TimelineEvent {
    type Header = (
        u8,
        DataHeaderOf<EventDuration>,
        DataHeaderOf<ActionInstance>,
    );
    const LATEST_HEADER: Self::Header = (
        1,
        EventDuration::LATEST_HEADER,
        ActionInstance::LATEST_HEADER,
    );

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += self.duration.write_data(writer)?;
        written += self.timeout.write_data(writer)?;
        written += self.actions.0.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if header.0 != Self::LATEST_HEADER.0 {
            return Err(DataReadError::unsupported(
                "TimelineEvent",
                Self::LATEST_HEADER.0,
                header.0,
            ));
        }

        Ok(Self {
            duration: reader.read_data(&header.1)?,
            timeout: reader.read_data(&header.1)?,
            actions: Actions(reader.read_data(&header.2)?),
        })
    }
}

impl DataFormat for EventDuration {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;

        match self {
            Self::Time(duration) => {
                written += 0u8.write_data(writer)?;
                written += duration.write_data(writer)?;
            }
            Self::Blocks(blocks) => {
                written += 1u8.write_data(writer)?;
                written += blocks.write_data(writer)?;
            }
        }

        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "EventDuration",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        match reader.read_data(&())? {
            0 => Ok(Self::Time(reader.read_data(&())?)),
            1 => Ok(Self::Blocks(reader.read_data(&())?)),
            _ => Err(DataReadError::custom("invalid EventDuration variant")),
        }
    }
}

impl DataFormat for ActionInstance {
    type Header = (u8, DataHeaderOf<Action>);
    const LATEST_HEADER: Self::Header = (1, Action::LATEST_HEADER);

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(self.awaited.write_data(writer)? + self.action.write_data(writer)?)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if header.0 != Self::LATEST_HEADER.0 {
            return Err(DataReadError::unsupported(
                "ActionInstance",
                Self::LATEST_HEADER.0,
                header.0,
            ));
        }

        Ok(Self {
            awaited: reader.read_data(&())?,
            action: reader.read_data(&header.1)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ActionFormatHeader {
    pub version: u8,
    pub node_targets: DataHeaderOf<NodeTargets>,
    pub spawn_cannon: DataHeaderOf<SpawnCannon>,
    pub reconfig: DataHeaderOf<Reconfig>,
}

impl DataFormat for ActionFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += self.version.write_data(writer)?;
        written += self.node_targets.write_data(writer)?;
        written += write_dataformat(writer, &self.spawn_cannon)?;
        written += write_dataformat(writer, &self.reconfig)?;
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
                "ActionFormatHeader",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        Ok(Self {
            version: reader.read_data(&())?,
            node_targets: reader.read_data(&((), ()))?,
            spawn_cannon: read_dataformat(reader)?,
            reconfig: read_dataformat(reader)?,
        })
    }
}

impl DataFormat for Action {
    type Header = ActionFormatHeader;
    const LATEST_HEADER: Self::Header = ActionFormatHeader {
        version: 1,
        node_targets: NodeTargets::LATEST_HEADER,
        spawn_cannon: SpawnCannon::LATEST_HEADER,
        reconfig: Reconfig::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(match self {
            Self::Online(targets) => 0u8.write_data(writer)? + targets.write_data(writer)?,
            Self::Offline(targets) => 1u8.write_data(writer)? + targets.write_data(writer)?,
            Self::Cannon(cannons) => 2u8.write_data(writer)? + cannons.write_data(writer)?,
            Self::Config(configs) => 3u8.write_data(writer)? + configs.write_data(writer)?,
        })
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(DataReadError::unsupported(
                "Action",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        match reader.read_data(&())? {
            0 => Ok(Self::Online(reader.read_data(&header.node_targets)?)),
            1 => Ok(Self::Offline(reader.read_data(&header.node_targets)?)),
            2 => Ok(Self::Cannon(reader.read_data(&header.spawn_cannon)?)),
            3 => Ok(Self::Config(
                reader.read_data(&(header.node_targets, header.reconfig.clone()))?,
            )),
            _ => Err(DataReadError::custom("unknown Action enum variant")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpawnCannonFormatHeader {
    pub version: u8,
    pub node_key: DataHeaderOf<NodeKey>,
    pub node_targets: DataHeaderOf<NodeTargets>,
}

impl DataFormat for SpawnCannonFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(self.version.write_data(writer)?
            + self.node_key.write_data(writer)?
            + self.node_targets.write_data(writer)?)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "SpawnCannonFormatHeader",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        Ok(Self {
            version: reader.read_data(&())?,
            node_key: reader.read_data(&((), ()))?,
            node_targets: reader.read_data(&((), ()))?,
        })
    }
}

impl DataFormat for SpawnCannon {
    type Header = SpawnCannonFormatHeader;
    const LATEST_HEADER: Self::Header = SpawnCannonFormatHeader {
        version: 1,
        node_key: NodeKey::LATEST_HEADER,
        node_targets: NodeTargets::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += self.name.write_data(writer)?;
        written += self.count.write_data(writer)?;
        written += self.query.write_data(writer)?;
        written += self.target.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(DataReadError::unsupported(
                "SpawnCannon",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        Ok(Self {
            name: reader.read_data(&())?,
            count: reader.read_data(&())?,
            query: reader.read_data(&header.node_key)?,
            target: reader.read_data(&header.node_targets)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ReconfigFormatHeader {
    pub version: u8,
    pub doc_height_request: DataHeaderOf<DocHeightRequest>,
    pub node_targets: DataHeaderOf<NodeTargets>,
}

impl DataFormat for ReconfigFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(self.version.write_data(writer)?
            + self.doc_height_request.write_data(writer)?
            + self.node_targets.write_data(writer)?)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "DataFormat",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        Ok(Self {
            version: reader.read_data(&())?,
            doc_height_request: reader.read_data(&((), ()))?,
            node_targets: reader.read_data(&((), ()))?,
        })
    }
}

impl DataFormat for Reconfig {
    type Header = ReconfigFormatHeader;
    const LATEST_HEADER: Self::Header = ReconfigFormatHeader {
        version: 1,
        doc_height_request: DocHeightRequest::LATEST_HEADER,
        node_targets: NodeTargets::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += self.height.write_data(writer)?;
        written += self.peers.write_data(writer)?;
        written += self.validators.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(DataReadError::unsupported(
                "Reconfig",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        Ok(Self {
            height: reader.read_data(&header.doc_height_request)?,
            peers: reader.read_data(&header.node_targets)?,
            validators: reader.read_data(&header.node_targets)?,
        })
    }
}
