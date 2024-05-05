use snops_common::{
    format::{read_dataformat, write_dataformat, DataFormat, DataFormatReader, DataFormatWriter},
    state::AgentId,
};

use crate::schema::nodes::{ExternalNode, Node, NodeFormatHeader};

#[derive(Debug, Clone)]
pub struct PersistNodeFormatHeader {
    pub(crate) node: NodeFormatHeader,
    pub(crate) external_node: <ExternalNode as DataFormat>::Header,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistNode {
    Internal(AgentId, Box<Node>),
    External(ExternalNode),
}

impl DataFormat for PersistNodeFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        Ok(write_dataformat(writer, &self.node)? + write_dataformat(writer, &self.external_node)?)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
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
        external_node: <ExternalNode as DataFormat>::LATEST_HEADER,
    };

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
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

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
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
            n => Err(snops_common::format::DataReadError::Custom(format!(
                "invalid PersistNode discriminant: {n}"
            ))),
        }
    }
}
