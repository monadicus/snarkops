use std::{collections::HashMap, net::SocketAddr};

use super::{AgentId, HeightRequest, NodeKey};
use crate::format::{DataFormat, DataFormatReader, PackedUint};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeState {
    pub node_key: NodeKey,
    pub private_key: KeyState,
    /// Increment the usize whenever the request is updated.
    pub height: (usize, HeightRequest),

    pub online: bool,
    pub peers: Vec<AgentPeer>,
    pub validators: Vec<AgentPeer>,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct NodeStateFormatHeader {
    version: u8,
    node_key: <NodeKey as DataFormat>::Header,
    key_state: <KeyState as DataFormat>::Header,
    height: <HeightRequest as DataFormat>::Header,
    peer: <AgentPeer as DataFormat>::Header,
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
        version: 1,
        node_key: <NodeKey as DataFormat>::LATEST_HEADER,
        key_state: <KeyState as DataFormat>::LATEST_HEADER,
        height: <HeightRequest as DataFormat>::LATEST_HEADER,
        peer: <AgentPeer as DataFormat>::LATEST_HEADER,
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
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(crate::format::DataReadError::unsupported(
                "NodeState",
                Self::LATEST_HEADER.version,
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

        Ok(NodeState {
            node_key,
            private_key,
            height: (height_inc.into(), height_req),
            online,
            peers,
            validators,
            env,
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
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub enum AgentPeer {
    Internal(AgentId, u16),
    External(SocketAddr),
}

impl AgentPeer {
    /// Get the port from the peer
    pub fn port(&self) -> u16 {
        match self {
            Self::Internal(_, port) => *port,
            Self::External(addr) => addr.port(),
        }
    }

    /// Return a new peer with the given port.
    pub fn with_port(&self, port: u16) -> Self {
        match self {
            Self::Internal(ip, _) => Self::Internal(*ip, port),
            Self::External(addr) => Self::External(SocketAddr::new(addr.ip(), port)),
        }
    }
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
