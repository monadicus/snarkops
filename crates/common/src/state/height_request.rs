use std::{fmt::Display, str::FromStr};

use snops_checkpoint::RetentionSpan;

use crate::format::{DataFormat, DataFormatReader, DataHeaderOf, DataReadError};

impl FromStr for HeightRequest {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "top" => Ok(HeightRequest::Top),
            s => {
                if let Ok(height) = s.parse() {
                    Ok(HeightRequest::Absolute(height))
                } else if let Ok(span) = s.parse() {
                    Ok(HeightRequest::Checkpoint(span))
                } else {
                    Err(format!("invalid HeightRequest: {}", s))
                }
            }
        }
    }
}

impl Display for HeightRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeightRequest::Top => write!(f, "top"),
            HeightRequest::Absolute(h) => write!(f, "{h}"),
            HeightRequest::Checkpoint(c) => write!(f, "{c}"),
        }
    }
}

#[derive(Debug, Default, Copy, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase", untagged)]
pub enum HeightRequest {
    #[default]
    /// Use the latest height for the ledger
    #[serde(with = "super::strings::top")]
    Top,
    /// Set the height to the given block (there must be a checkpoint at this
    /// height) Setting to 0 will reset the height to the genesis block
    Absolute(u32),
    /// Use the next checkpoint that matches this checkpoint span
    Checkpoint(snops_checkpoint::RetentionSpan),
    // the control plane doesn't know the heights the nodes are at
    // TruncateHeight(u32),
    // TruncateTime(i64),
}

impl DataFormat for HeightRequest {
    type Header = (u8, DataHeaderOf<RetentionSpan>);
    const LATEST_HEADER: Self::Header = (1, RetentionSpan::LATEST_HEADER);

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        match self {
            HeightRequest::Top => 0u8.write_data(writer),
            HeightRequest::Absolute(height) => {
                Ok(1u8.write_data(writer)? + height.write_data(writer)?)
            }
            HeightRequest::Checkpoint(retention) => {
                Ok(2u8.write_data(writer)? + retention.write_data(writer)?)
            }
        }
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, DataReadError> {
        if header.0 != Self::LATEST_HEADER.0 {
            return Err(DataReadError::unsupported(
                "HeightRequest",
                Self::LATEST_HEADER.0,
                header.0,
            ));
        }
        match reader.read_data(&())? {
            0u8 => Ok(HeightRequest::Top),
            1u8 => Ok(HeightRequest::Absolute(reader.read_data(&())?)),
            2u8 => Ok(HeightRequest::Checkpoint(reader.read_data(&header.1)?)),
            n => Err(DataReadError::Custom(format!(
                "invalid HeightRequest discriminant: {n}"
            ))),
        }
    }
}

impl HeightRequest {
    pub fn is_top(&self) -> bool {
        *self == Self::Top
    }

    pub fn reset(&self) -> bool {
        // height 0 = genesis block
        // checkpoint an unlimited time in the past is also a reset
        *self == Self::Absolute(0) || *self == Self::Checkpoint(RetentionSpan::Unlimited)
    }
}
