use snops_common::{node_targets::NodeTargets, state::TxPipeId};

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
        version: 2,
        node_targets: NodeTargets::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += self.file_name.write_data(writer)?;
        written += self.target.write_data(writer)?;
        written += self.broadcast_attempts.write_data(writer)?;
        written += self.authorize_attempts.write_data(writer)?;
        written += self.broadcast_timeout.write_data(writer)?;
        written += self.authorize_timeout.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        match header.version {
            1u8 => match reader.read_data(&())? {
                0u8 => {
                    let file_name: TxPipeId = reader.read_data(&())?;
                    Ok(TxSink {
                        file_name: Some(file_name),
                        target: None,
                        broadcast_attempts: None,
                        authorize_attempts: None,
                        broadcast_timeout: TxSink::default_retry_timeout(),
                        authorize_timeout: TxSink::default_retry_timeout(),
                    })
                }
                1u8 => {
                    let target: NodeTargets = reader.read_data(&header.node_targets)?;
                    Ok(TxSink {
                        target: Some(target),
                        file_name: None,
                        broadcast_attempts: None,
                        authorize_attempts: None,
                        broadcast_timeout: TxSink::default_retry_timeout(),
                        authorize_timeout: TxSink::default_retry_timeout(),
                    })
                }
                n => Err(DataReadError::Custom(format!(
                    "invalid TxSink discriminant: {n}"
                ))),
            },
            n if n == Self::LATEST_HEADER.version => {
                let file_name: Option<TxPipeId> = reader.read_data(&())?;
                let target: Option<NodeTargets> = reader.read_data(&header.node_targets)?;
                let broadcast_attempts: Option<u32> = reader.read_data(&())?;
                let authorize_attempts: Option<u32> = reader.read_data(&())?;
                let broadcast_timeout: u32 = reader.read_data(&())?;
                let authorize_timeout: u32 = reader.read_data(&())?;
                Ok(TxSink {
                    file_name,
                    target,
                    broadcast_attempts,
                    authorize_attempts,
                    broadcast_timeout,
                    authorize_timeout,
                })
            }
            n => Err(DataReadError::unsupported(
                "TxSink",
                format!("1 or {}", Self::LATEST_HEADER.version),
                n,
            )),
        }
    }
}
