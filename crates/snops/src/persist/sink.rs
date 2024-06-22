use snops_common::node_targets::NodeTargets;

use super::prelude::*;
use crate::cannon::sink::TxSink;

#[derive(Debug, Clone)]
pub struct TxSinkFormatHeader {
    pub version: u8,
    pub node_targets: DataHeaderOf<NodeTargets>,
}

impl DataFormat for TxSinkFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(self.version.write_data(writer)? + write_dataformat(writer, &self.node_targets)?)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "TxSinkFormatHeader",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        let version = reader.read_data(&())?;
        let node_targets = read_dataformat(reader)?;

        Ok(Self {
            version,
            node_targets,
        })
    }
}

impl DataFormat for TxSink {
    type Header = TxSinkFormatHeader;
    const LATEST_HEADER: Self::Header = TxSinkFormatHeader {
        version: 1,
        node_targets: NodeTargets::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        match self {
            TxSink::Record { file_name } => {
                written += 0u8.write_data(writer)?;
                written += file_name.write_data(writer)?;
            }
            TxSink::RealTime { target } => {
                written += 1u8.write_data(writer)?;
                written += target.write_data(writer)?;
            }
        }

        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(DataReadError::unsupported(
                "TxSink",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        match reader.read_data(&())? {
            0u8 => {
                let file_name = reader.read_data(&())?;
                Ok(TxSink::Record { file_name })
            }
            1u8 => {
                let target = reader.read_data(&header.node_targets)?;
                Ok(TxSink::RealTime { target })
            }
            n => Err(DataReadError::Custom(format!(
                "invalid TxSink discriminant: {n}"
            ))),
        }
    }
}
