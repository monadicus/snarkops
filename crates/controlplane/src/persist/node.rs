use snops_common::state::AgentId;

use super::prelude::*;
use crate::schema::nodes::{ExternalNode, Node, NodeFormatHeader};

#[derive(Debug, Clone)]
pub struct PersistNodeFormatHeader {
    pub(crate) node: NodeFormatHeader,
    pub(crate) external_node: DataHeaderOf<ExternalNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistNode {
    Internal(AgentId, Box<Node>),
    External(ExternalNode),
}

impl DataFormat for PersistNodeFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

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

        Ok(PersistNodeFormatHeader {
            node,
            external_node,
        })
    }
}

impl DataFormat for PersistNode {
    type Header = PersistNodeFormatHeader;
    const LATEST_HEADER: Self::Header = PersistNodeFormatHeader {
        node: Node::LATEST_HEADER,
        external_node: ExternalNode::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        match self {
            PersistNode::Internal(id, state) => {
                written += writer.write_data(&0u8)?;
                written += writer.write_data(id)?;
                written += writer.write_data(state)?;
            }
            PersistNode::External(n) => {
                written += writer.write_data(&1u8)?;
                written += writer.write_data(n)?;
            }
        }
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        match reader.read_data(&())? {
            0u8 => {
                let id = reader.read_data(&())?;
                let state = reader.read_data(&header.node)?;
                Ok(PersistNode::Internal(id, Box::new(state)))
            }
            1u8 => {
                let n = reader.read_data(&header.external_node)?;
                Ok(PersistNode::External(n))
            }
            n => Err(DataReadError::Custom(format!(
                "invalid PersistNode discriminant: {n}"
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
        state::{HeightRequest, InternedId},
    };

    use crate::{
        persist::{PersistNode, PersistNodeFormatHeader},
        schema::nodes::{ExternalNode, Node, NodeFormatHeader},
    };

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
        PersistNodeFormatHeader,
        PersistNode::LATEST_HEADER,
        [
            NodeFormatHeader::LATEST_HEADER.to_byte_vec()?,
            Node::LATEST_HEADER.to_byte_vec()?,
            ExternalNode::LATEST_HEADER.to_byte_vec()?,
        ]
        .concat()
    );

    case!(
        node_internal,
        PersistNode,
        PersistNode::Internal(
            InternedId::from_str("id")?,
            Box::new(Node {
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
            })
        ),
        [
            0u8.to_byte_vec()?,
            InternedId::from_str("id")?.to_byte_vec()?,
            Node {
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
        PersistNode,
        PersistNode::External(ExternalNode {
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
