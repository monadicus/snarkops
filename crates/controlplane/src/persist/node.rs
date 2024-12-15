use snops_common::schema::{
    nodes::{ExternalNode, NodeDoc},
    persist::NodeFormatHeader,
};

use super::prelude::*;
use crate::env::EnvNode;

#[derive(Debug, Clone)]
pub struct EnvNodeStateFormatHeader {
    pub(crate) version: u8,
    pub(crate) node: NodeFormatHeader,
    pub(crate) external_node: DataHeaderOf<ExternalNode>,
}

impl DataFormat for EnvNodeStateFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 2;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(write_dataformat(writer, &self.node)? + write_dataformat(writer, &self.external_node)?)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "PersistNodeFormatHeader",
                Self::LATEST_HEADER,
                header,
            ));
        }

        let node = read_dataformat(reader)?;
        let external_node = read_dataformat(reader)?;

        Ok(EnvNodeStateFormatHeader {
            version: *header,
            node,
            external_node,
        })
    }
}

impl DataFormat for EnvNode {
    type Header = EnvNodeStateFormatHeader;
    const LATEST_HEADER: Self::Header = EnvNodeStateFormatHeader {
        version: EnvNodeStateFormatHeader::LATEST_HEADER,
        node: NodeDoc::LATEST_HEADER,
        external_node: ExternalNode::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        match self {
            EnvNode::Internal { agent, node } => {
                written += writer.write_data(&0u8)?;
                written += writer.write_data(agent)?;
                written += writer.write_data(node)?;
            }
            EnvNode::External(n) => {
                written += writer.write_data(&1u8)?;
                written += writer.write_data(n)?;
            }
        }
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        match reader.read_data(&())? {
            0u8 => {
                let agent = if header.version == 1 {
                    // Version 1 required an agent id, later versions have the agent id as an option
                    Some(reader.read_data(&())?)
                } else {
                    reader.read_data(&())?
                };
                let node = reader.read_data(&header.node)?;
                Ok(EnvNode::Internal { agent, node })
            }
            1u8 => {
                let n = reader.read_data(&header.external_node)?;
                Ok(EnvNode::External(n))
            }
            n => Err(DataReadError::Custom(format!(
                "invalid EnvNodeState discriminant: {n}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use snops_common::{
        format::DataFormat,
        node_targets::NodeTargets,
        schema::{
            nodes::{ExternalNode, NodeDoc},
            persist::NodeFormatHeader,
        },
        state::{HeightRequest, InternedId},
    };

    use crate::{env::EnvNode, persist::EnvNodeStateFormatHeader};

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr, $b:expr) => {
            #[test]
            fn $name() -> Result<(), Box<dyn std::error::Error>> {
                let mut data = Vec::new();
                let value: $ty = $a;
                value.write_data(&mut data)?;
                assert_eq!(data, $b);

                let mut reader = &data[..];
                let read_value =
                    <$ty>::read_data(&mut reader, &<$ty as DataFormat>::LATEST_HEADER)?;

                let mut rewritten = Vec::new();
                read_value.write_data(&mut rewritten)?;
                assert_eq!(data, rewritten);
                Ok(())
            }
        };
    }

    case!(
        node_header,
        EnvNodeStateFormatHeader,
        EnvNode::LATEST_HEADER,
        [
            NodeFormatHeader::LATEST_HEADER.to_byte_vec()?,
            NodeDoc::LATEST_HEADER.to_byte_vec()?,
            ExternalNode::LATEST_HEADER.to_byte_vec()?,
        ]
        .concat()
    );

    case!(
        node_internal,
        EnvNode,
        EnvNode::Internal {
            agent: Some(InternedId::from_str("id")?),
            node: NodeDoc {
                online: true,
                replicas: None,
                key: None,
                height: HeightRequest::Top,
                labels: Default::default(),
                agent: None,
                validators: NodeTargets::None,
                peers: NodeTargets::None,
                env: Default::default(),
                binary: None,
            }
        },
        [
            0u8.to_byte_vec()?,
            Some(InternedId::from_str("id")?).to_byte_vec()?,
            NodeDoc {
                online: true,
                replicas: None,
                key: None,
                height: HeightRequest::Top,
                labels: Default::default(),
                agent: None,
                validators: NodeTargets::None,
                peers: NodeTargets::None,
                env: Default::default(),
                binary: None,
            }
            .to_byte_vec()?,
        ]
        .concat()
    );

    case!(
        node_external,
        EnvNode,
        EnvNode::External(ExternalNode {
            bft: None,
            node: None,
            rest: None
        }),
        [
            1u8.to_byte_vec()?,
            ExternalNode {
                bft: None,
                node: None,
                rest: None
            }
            .to_byte_vec()?,
        ]
        .concat()
    );
}
