use snops_common::{key_source::KeySource, node_targets::NodeTargets};

use super::prelude::*;
use crate::cannon::source::{ComputeTarget, LocalService, QueryTarget, TxSource};

#[derive(Debug, Clone)]
pub struct TxSourceFormatHeader {
    pub version: u8,
    pub node_targets: DataHeaderOf<NodeTargets>,
    pub key_source: DataHeaderOf<KeySource>,
}

impl DataFormat for TxSourceFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(self.version.write_data(writer)?
            + self.node_targets.write_data(writer)?
            + self.key_source.write_data(writer)?)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "LocalServiceFormatHeader",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        let version = reader.read_data(&())?;
        let node_targets = reader.read_data(&((), ()))?;
        let key_source = reader.read_data(&())?;
        Ok(Self {
            version,
            node_targets,
            key_source,
        })
    }
}

impl DataFormat for TxSource {
    type Header = TxSourceFormatHeader;
    const LATEST_HEADER: Self::Header = TxSourceFormatHeader {
        version: 1,
        node_targets: NodeTargets::LATEST_HEADER,
        key_source: KeySource::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += 2u8.write_data(writer)?;

        match &self.query {
            QueryTarget::Local(local) => {
                written += 0u8.write_data(writer)?;
                written += local.sync_from.write_data(writer)?;
            }
            QueryTarget::Node(node) => {
                written += 1u8.write_data(writer)?;
                written += node.write_data(writer)?;
            }
        }

        match &self.compute {
            ComputeTarget::Agent { labels } => {
                written += 0u8.write_data(writer)?;
                written += labels.write_data(writer)?;
            }
            ComputeTarget::Demox { demox_api } => {
                written += 1u8.write_data(writer)?;
                written += demox_api.write_data(writer)?;
            }
        }

        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(DataReadError::unsupported(
                "TxSource",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        let query = match reader.read_data(&())? {
            0u8 => QueryTarget::Local(LocalService {
                sync_from: reader.read_data(&header.node_targets)?,
            }),
            1u8 => QueryTarget::Node(reader.read_data(&header.node_targets)?),
            n => {
                return Err(DataReadError::Custom(format!(
                    "invalid QueryTarget discriminant: {n}"
                )));
            }
        };

        let compute = match reader.read_data(&())? {
            0u8 => ComputeTarget::Agent {
                labels: reader.read_data(&())?,
            },
            1u8 => ComputeTarget::Demox {
                demox_api: reader.read_data(&())?,
            },
            n => {
                return Err(DataReadError::Custom(format!(
                    "invalid ComputeTarget discriminant: {n}"
                )));
            }
        };

        Ok(TxSource { query, compute })
    }
}
