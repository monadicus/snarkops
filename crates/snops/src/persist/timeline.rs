use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::SystemTime,
};

use atomic_time::AtomicSystemTime;
use snops_common::state::TimelineId;

use super::prelude::*;
use crate::{
    env::timeline::TimelineInstance,
    schema::{
        outcomes::{OutcomeExpectation, OutcomeValidation},
        timeline::{OutcomeMetrics, TimelineEvent},
    },
    util::OpaqueDebug,
};

pub struct PersistTimelineInstance {
    pub id: TimelineId,
    pub events: Vec<TimelineEvent>,
    pub outcomes: OutcomeMetrics,
    pub step: usize,
    pub step_wait_until: Option<SystemTime>,
}

impl PersistTimelineInstance {
    pub fn from_instance(value: &TimelineInstance) -> Self {
        let step_wait_until = value.step_wait_until.load(Ordering::Acquire);

        Self {
            id: value.id,
            events: value.events.clone(),
            outcomes: value.outcomes.clone(),
            step: value.step.load(Ordering::Acquire),
            step_wait_until: match step_wait_until {
                SystemTime::UNIX_EPOCH => None,
                other => Some(other),
            },
        }
    }
}

impl From<PersistTimelineInstance> for TimelineInstance {
    fn from(value: PersistTimelineInstance) -> Self {
        Self {
            id: value.id,
            events: value.events,
            outcomes: value.outcomes,
            handle: Default::default(),
            step: AtomicUsize::new(value.step),
            step_mutex: Default::default(),
            step_wait_until: OpaqueDebug(AtomicSystemTime::new(
                value.step_wait_until.unwrap_or(SystemTime::UNIX_EPOCH),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PersistTimelineInstanceFormatHeader {
    pub version: u8,
    pub event: DataHeaderOf<TimelineEvent>,
    pub outcome: DataHeaderOf<OutcomeExpectation>,
}

impl DataFormat for PersistTimelineInstanceFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += self.version.write_data(writer)?;
        written += write_dataformat(writer, &self.event)?;
        written += write_dataformat(writer, &self.outcome)?;
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "PersistTimelineInstanceFormatHeader",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        Ok(Self {
            version: reader.read_data(&())?,
            event: read_dataformat(reader)?,
            outcome: read_dataformat(reader)?,
        })
    }
}

impl DataFormat for PersistTimelineInstance {
    type Header = PersistTimelineInstanceFormatHeader;
    const LATEST_HEADER: Self::Header = PersistTimelineInstanceFormatHeader {
        version: 1,
        event: TimelineEvent::LATEST_HEADER,
        outcome: OutcomeExpectation::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += self.id.write_data(writer)?;
        written += self.events.write_data(writer)?;
        written += self.outcomes.write_data(writer)?;
        written += self.step.write_data(writer)?;
        written += self.step_wait_until.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(DataReadError::unsupported(
                "PersistTimelineInstance",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        Ok(Self {
            id: reader.read_data(&())?,
            events: reader.read_data(&header.event)?,
            outcomes: reader.read_data(&((), header.outcome))?,
            step: reader.read_data(&())?,
            step_wait_until: reader.read_data(&())?,
        })
    }
}

impl DataFormat for OutcomeExpectation {
    type Header = DataHeaderOf<OutcomeValidation>;
    const LATEST_HEADER: Self::Header = OutcomeValidation::LATEST_HEADER;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;

        written += self.query.write_data(writer)?;
        written += self.validation.write_data(writer)?;

        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        Ok(Self {
            query: reader.read_data(&())?,
            validation: reader.read_data(header)?,
        })
    }
}

impl DataFormat for OutcomeValidation {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;

        match self {
            Self::Range { min, max } => {
                written += 0u8.write_data(writer)?;
                written += min.write_data(writer)?;
                written += max.write_data(writer)?;
            }
            Self::Eq { eq, epsilon } => {
                written += 1u8.write_data(writer)?;
                written += eq.write_data(writer)?;
                written += epsilon.write_data(writer)?;
            }
        }

        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
                "OutcomeValidation",
                Self::LATEST_HEADER,
                header,
            ));
        }

        Ok(match reader.read_data(&())? {
            0 => Self::Range {
                min: reader.read_data(&())?,
                max: reader.read_data(&())?,
            },
            1 => Self::Eq {
                eq: reader.read_data(&())?,
                epsilon: reader.read_data(&())?,
            },
            _ => return Err(DataReadError::custom("invalid validation enum variant")),
        })
    }
}
