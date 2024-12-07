use lasso::Spur;

use crate::schema::nodes::{ExternalNode, Node};
use crate::{
    format::{
        DataFormat, DataFormatReader, DataFormatWriter, DataHeaderOf, DataReadError, DataWriteError,
    },
    key_source::KeySource,
    node_targets::NodeTargets,
    state::HeightRequest,
};

impl DataFormat for ExternalNode {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += writer.write_data(&self.bft)?;
        written += writer.write_data(&self.node)?;
        written += writer.write_data(&self.rest)?;
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, DataReadError> {
        match header {
            1 => {
                let bft = reader.read_data(&())?;
                let node = reader.read_data(&())?;
                let rest = reader.read_data(&())?;
                Ok(ExternalNode { bft, node, rest })
            }
            _ => Err(DataReadError::Custom("unsupported version".to_owned())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeFormatHeader {
    pub(crate) key_source: DataHeaderOf<KeySource>,
    pub(crate) height_request: DataHeaderOf<HeightRequest>,
    pub(crate) node_targets: DataHeaderOf<NodeTargets>,
    pub has_binaries: bool,
}

impl DataFormat for NodeFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 2;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += self.key_source.write_data(writer)?;
        written += self.height_request.write_data(writer)?;
        written += self.node_targets.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, DataReadError> {
        if *header == 0 || *header > Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "NodeFormatHeader",
                format!("1 or {}", Self::LATEST_HEADER),
                *header,
            ));
        }

        let key_source = KeySource::read_header(reader)?;
        let height_request = HeightRequest::read_header(reader)?;
        let node_targets = NodeTargets::read_header(reader)?;
        Ok(NodeFormatHeader {
            key_source,
            height_request,
            node_targets,
            has_binaries: *header > 1,
        })
    }
}

impl DataFormat for Node {
    type Header = NodeFormatHeader;
    const LATEST_HEADER: Self::Header = NodeFormatHeader {
        key_source: KeySource::LATEST_HEADER,
        height_request: HeightRequest::LATEST_HEADER,
        node_targets: NodeTargets::LATEST_HEADER,
        has_binaries: true,
    };

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += self.online.write_data(writer)?;
        written += self.replicas.write_data(writer)?;
        written += self.key.write_data(writer)?;
        written += self.height.write_data(writer)?;
        written += self.labels.write_data(writer)?;
        written += self.agent.write_data(writer)?;
        written += self.validators.write_data(writer)?;
        written += self.peers.write_data(writer)?;
        written += self.env.write_data(writer)?;
        written += self.binary.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, DataReadError> {
        let online = reader.read_data(&())?;
        let replicas = reader.read_data(&())?;
        let key = reader.read_data(&header.key_source)?;
        let height = reader.read_data(&header.height_request)?;
        let labels = Vec::<Spur>::read_data(reader, &())?;
        let agent = reader.read_data(&())?;
        let validators = reader.read_data(&header.node_targets)?;
        let peers = reader.read_data(&header.node_targets)?;
        let env = Vec::<(String, String)>::read_data(reader, &((), ()))?;
        let binary = if header.has_binaries {
            reader.read_data(&())?
        } else {
            None
        };

        Ok(Node {
            online,
            replicas,
            key,
            height,
            labels: labels.into_iter().collect(),
            agent,
            validators,
            peers,
            env: env.into_iter().collect(),
            binary,
        })
    }
}
