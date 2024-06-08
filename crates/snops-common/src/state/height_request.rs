use std::str::FromStr;

use checkpoint::RetentionSpan;

use crate::format::{DataFormat, DataFormatReader, DataHeaderOf, DataReadError};

/// for some reason bincode does not allow deserialize_any so if i want to allow
/// end users to type "top", 42, or "persist" i need to do have to copies of
/// this where one is not untagged.
///
/// bincode. please.
#[derive(Debug, Copy, Default, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase", untagged)]
pub enum DocHeightRequest {
    #[default]
    /// Use the latest height for the ledger
    #[serde(with = "super::strings::top")]
    Top,
    /// Set the height to the given block (there must be a checkpoint at this
    /// height) Setting to 0 will reset the height to the genesis block
    Absolute(u32),
    /// Use the next checkpoint that matches this checkpoint span
    Checkpoint(checkpoint::RetentionSpan),
    // the control plane doesn't know the heights the nodes are at
    // TruncateHeight(u32),
    // TruncateTime(i64),
}

impl FromStr for DocHeightRequest {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "top" => Ok(DocHeightRequest::Top),
            s => {
                if let Ok(height) = s.parse() {
                    Ok(DocHeightRequest::Absolute(height))
                } else if let Ok(span) = s.parse() {
                    Ok(DocHeightRequest::Checkpoint(span))
                } else {
                    Err(format!("invalid DocHeightRequest: {}", s))
                }
            }
        }
    }
}

impl DataFormat for DocHeightRequest {
    type Header = (u8, DataHeaderOf<RetentionSpan>);
    const LATEST_HEADER: Self::Header = (1, RetentionSpan::LATEST_HEADER);

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        match self {
            DocHeightRequest::Top => 0u8.write_data(writer),
            DocHeightRequest::Absolute(height) => {
                Ok(1u8.write_data(writer)? + height.write_data(writer)?)
            }
            DocHeightRequest::Checkpoint(retention) => {
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
                "DocHeightRequest",
                Self::LATEST_HEADER.0,
                header.0,
            ));
        }
        match reader.read_data(&())? {
            0u8 => Ok(DocHeightRequest::Top),
            1u8 => Ok(DocHeightRequest::Absolute(reader.read_data(&())?)),
            2u8 => Ok(DocHeightRequest::Checkpoint(reader.read_data(&header.1)?)),
            n => Err(DataReadError::Custom(format!(
                "invalid DocHeightRequest discrminant: {n}"
            ))),
        }
    }
}

#[derive(Debug, Default, Copy, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HeightRequest {
    #[default]
    /// Use the latest height for the ledger
    Top,
    /// Set the height to the given block (there must be a checkpoint at this
    /// height) Setting to 0 will reset the height to the genesis block
    Absolute(u32),
    /// Use the next checkpoint that matches this checkpoint span
    Checkpoint(checkpoint::RetentionSpan),
    // the control plane doesn't know the heights the nodes are at
    // TruncateHeight(u32),
    // TruncateTime(i64),
}

// TODO: now that we don't use bincode for storage format, we should be able to
// make remove HeightRequest and rename DocHeightRequest to HeightRequest
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
                "invalid HeightRequest discrminant: {n}"
            ))),
        }
    }
}

impl HeightRequest {
    pub fn is_top(&self) -> bool {
        *self == Self::Top
    }

    pub fn reset(&self) -> bool {
        *self == Self::Absolute(0)
    }
}

impl From<DocHeightRequest> for HeightRequest {
    fn from(req: DocHeightRequest) -> Self {
        match req {
            DocHeightRequest::Top => Self::Top,
            DocHeightRequest::Absolute(h) => Self::Absolute(h),
            DocHeightRequest::Checkpoint(c) => Self::Checkpoint(c),
        }
    }
}
