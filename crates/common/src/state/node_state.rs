use std::net::SocketAddr;

use indexmap::IndexMap;

use super::{AgentId, HeightRequest, InternedId, NodeKey};
use crate::format::{DataFormat, DataFormatReader, DataHeaderOf, PackedUint};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeState {
    pub node_key: NodeKey,
    pub private_key: KeyState,
    /// Increment the usize whenever the request is updated.
    pub height: (usize, HeightRequest),

    pub online: bool,
    pub peers: Vec<AgentPeer>,
    pub validators: Vec<AgentPeer>,
    pub env: IndexMap<String, String>,
    pub binary: Option<InternedId>,
}

#[derive(Debug, Clone)]
pub struct NodeStateFormatHeader {
    version: u8,
    node_key: DataHeaderOf<NodeKey>,
    key_state: DataHeaderOf<KeyState>,
    height: DataHeaderOf<HeightRequest>,
    peer: DataHeaderOf<AgentPeer>,
}

impl DataFormat for NodeStateFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        let mut written = 0;
        written += self.version.write_data(writer)?;
        written += self.node_key.write_data(writer)?;
        written += self.key_state.write_data(writer)?;
        written += self.height.write_data(writer)?;
        written += self.peer.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(crate::format::DataReadError::unsupported(
                "NodeStateFormatHeader",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        Ok(NodeStateFormatHeader {
            version: reader.read_data(&())?,
            node_key: reader.read_data(&((), ()))?,
            key_state: reader.read_data(&())?,
            height: reader.read_data(&((), ()))?,
            peer: reader.read_data(&())?,
        })
    }
}

impl DataFormat for NodeState {
    type Header = NodeStateFormatHeader;
    const LATEST_HEADER: Self::Header = NodeStateFormatHeader {
        version: 2,
        node_key: NodeKey::LATEST_HEADER,
        key_state: KeyState::LATEST_HEADER,
        height: HeightRequest::LATEST_HEADER,
        peer: AgentPeer::LATEST_HEADER,
    };

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        let mut written = 0;
        written += self.node_key.write_data(writer)?;
        written += self.private_key.write_data(writer)?;
        written += PackedUint::from(self.height.0).write_data(writer)?;
        written += self.height.1.write_data(writer)?;
        written += self.online.write_data(writer)?;
        written += self.peers.write_data(writer)?;
        written += self.validators.write_data(writer)?;
        written += self.env.write_data(writer)?;
        written += self.binary.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if header.version == 0 || header.version > Self::LATEST_HEADER.version {
            return Err(crate::format::DataReadError::unsupported(
                "NodeState",
                format!("1 or {}", Self::LATEST_HEADER.version),
                header.version,
            ));
        }

        let node_key = reader.read_data(&header.node_key)?;
        let private_key = reader.read_data(&header.key_state)?;
        let height_inc = PackedUint::read_data(reader, &())?;
        let height_req = reader.read_data(&header.height)?;
        let online = reader.read_data(&())?;
        let peers = reader.read_data(&header.peer)?;
        let validators = reader.read_data(&header.peer)?;
        let env = reader.read_data(&((), ()))?;
        let binary = if header.version > 1 {
            reader.read_data(&())?
        } else {
            None
        };

        Ok(NodeState {
            node_key,
            private_key,
            height: (height_inc.into(), height_req),
            online,
            peers,
            validators,
            env,
            binary,
        })
    }
}

/// A representation of which key to use for the agent.
#[derive(Default, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum KeyState {
    /// No private key provided
    #[default]
    None,
    /// A private key is provided by the agent
    Local,
    /// A literal private key
    Literal(String),
    // TODO: generated?/new
}

impl From<Option<String>> for KeyState {
    fn from(s: Option<String>) -> Self {
        match s {
            Some(s) => Self::Literal(s),
            None => Self::None,
        }
    }
}

impl KeyState {
    pub fn try_string(&self) -> Option<String> {
        match self {
            Self::Literal(s) => Some(s.to_owned()),
            _ => None,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, KeyState::None)
    }
}

/// Peers sent to the agent with resolved addresses or port numbers
#[derive(
    Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord,
)]
pub enum AgentPeer {
    Internal(AgentId, u16),
    External(SocketAddr),
}

impl DataFormat for KeyState {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        match self {
            Self::None => 0u8.write_data(writer),
            Self::Local => 1u8.write_data(writer),
            Self::Literal(s) => Ok(2u8.write_data(writer)? + s.write_data(writer)?),
        }
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(crate::format::DataReadError::unsupported(
                "KeyState",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        match reader.read_data(&())? {
            0u8 => Ok(Self::None),
            1u8 => Ok(Self::Local),
            2u8 => Ok(Self::Literal(reader.read_data(&())?)),
            n => Err(crate::format::DataReadError::Custom(format!(
                "Invalid KeyState discriminant: {n}",
            ))),
        }
    }
}

impl DataFormat for AgentPeer {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        match self {
            Self::Internal(id, port) => {
                Ok(0u8.write_data(writer)? + id.write_data(writer)? + port.write_data(writer)?)
            }
            Self::External(addr) => Ok(1u8.write_data(writer)? + addr.write_data(writer)?),
        }
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(crate::format::DataReadError::unsupported(
                "AgentPeer",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        match reader.read_data(&())? {
            0u8 => Ok(Self::Internal(
                reader.read_data(&())?,
                reader.read_data(&())?,
            )),
            1u8 => Ok(Self::External(reader.read_data(&())?)),
            n => Err(crate::format::DataReadError::Custom(format!(
                "Invalid AgentPeer discriminant: {n}",
            ))),
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        format::{read_dataformat, write_dataformat, DataFormat},
        prelude::{HeightRequest, KeyState, NodeState, NodeStateFormatHeader},
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
        node_state,
        NodeState,
        NodeState {
            node_key: "client/foo".parse()?,
            private_key: KeyState::None,
            height: (0, HeightRequest::Top),
            online: true,
            peers: vec![],
            validators: vec![],
            env: Default::default(),
            binary: None,
        },
        [
            NodeStateFormatHeader::LATEST_HEADER.to_byte_vec()?,
            NodeState::LATEST_HEADER.to_byte_vec()?,
            NodeState {
                node_key: "client/foo".parse()?,
                private_key: KeyState::None,
                height: (0, HeightRequest::Top),
                online: true,
                peers: vec![],
                validators: vec![],
                env: Default::default(),
                binary: None,
            }
            .to_byte_vec()?,
        ]
        .concat()
    );
}
