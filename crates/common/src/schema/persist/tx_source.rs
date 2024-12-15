use std::io::{Read, Write};

use crate::{
    format::{DataFormat, DataFormatReader, DataHeaderOf, DataReadError, DataWriteError},
    node_targets::NodeTargets,
    schema::cannon::source::{ComputeTarget, LocalService, QueryTarget, TxSource},
};

#[derive(Debug, Clone)]
pub struct TxSourceFormatHeader {
    pub version: u8,
    pub node_targets: DataHeaderOf<NodeTargets>,
}

impl DataFormat for TxSourceFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(self.version.write_data(writer)? + self.node_targets.write_data(writer)?)
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
        Ok(Self {
            version,
            node_targets,
        })
    }
}

impl DataFormat for TxSource {
    type Header = TxSourceFormatHeader;
    const LATEST_HEADER: Self::Header = TxSourceFormatHeader {
        version: 1,
        node_targets: NodeTargets::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;

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

#[cfg(test)]
mod tests {

    use crate::{
        format::{read_dataformat, write_dataformat, DataFormat},
        node_targets::NodeTargets,
        schema::{
            cannon::source::{ComputeTarget, LocalService, QueryTarget, TxSource},
            persist::TxSourceFormatHeader,
        },
        INTERN,
    };

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr, $b:expr) => {
            #[test]
            fn $name() -> Result<(), Box<dyn std::error::Error>> {
                let mut data = Vec::new();
                write_dataformat(&mut data, &$a)?;
                assert_eq!(data, $b);

                let mut reader = &data[..];
                let read_value = read_dataformat::<_, $ty>(&mut reader)?;

                // write the data again because not every type implements PartialEq
                let mut data2 = Vec::new();
                write_dataformat(&mut data2, &read_value)?;
                assert_eq!(data, data2);
                Ok(())
            }
        };
    }

    case!(
        source_header,
        TxSourceFormatHeader,
        TxSource::LATEST_HEADER,
        [
            TxSourceFormatHeader::LATEST_HEADER.to_byte_vec()?,
            TxSource::LATEST_HEADER.version.to_byte_vec()?,
            NodeTargets::LATEST_HEADER.to_byte_vec()?,
        ]
        .concat()
    );

    case!(
        source_local_local_none,
        TxSource,
        TxSource {
            query: QueryTarget::Local(LocalService { sync_from: None }),
            compute: ComputeTarget::Agent { labels: None }
        },
        [
            TxSourceFormatHeader::LATEST_HEADER.to_byte_vec()?,
            TxSource::LATEST_HEADER.to_byte_vec()?,
            0u8.to_byte_vec()?, // querytarget local discriminant
            0u8.to_byte_vec()?, // sync from empty option
            0u8.to_byte_vec()?, // computetarget agent discriminant
            0u8.to_byte_vec()?, // labels empty option
        ]
        .concat()
    );

    case!(
        source_local_local_some,
        TxSource,
        TxSource {
            query: QueryTarget::Local(LocalService {
                sync_from: Some(NodeTargets::One("client/*".parse()?))
            }),
            compute: ComputeTarget::Agent {
                labels: Some(vec![INTERN.get_or_intern("foo")])
            }
        },
        [
            TxSourceFormatHeader::LATEST_HEADER.to_byte_vec()?,
            TxSource::LATEST_HEADER.to_byte_vec()?,
            0u8.to_byte_vec()?, // querytarget local discriminant
            Some(NodeTargets::One("client/*".parse()?)).to_byte_vec()?,
            0u8.to_byte_vec()?, // computetarget agent discriminant
            Some(vec!["foo".to_owned()]).to_byte_vec()?,
        ]
        .concat()
    );

    case!(
        source_node_demox,
        TxSource,
        TxSource {
            query: QueryTarget::Node(NodeTargets::One("client/*".parse()?)),
            compute: ComputeTarget::Demox {
                demox_api: "foo".to_owned()
            }
        },
        [
            TxSourceFormatHeader::LATEST_HEADER.to_byte_vec()?,
            TxSource::LATEST_HEADER.to_byte_vec()?,
            1u8.to_byte_vec()?, // querytarget node discriminant
            NodeTargets::One("client/*".parse()?).to_byte_vec()?,
            1u8.to_byte_vec()?, // computetarget demox discriminant
            "foo".to_owned().to_byte_vec()?,
        ]
        .concat()
    );
}
