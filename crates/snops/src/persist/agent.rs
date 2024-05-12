use snops_common::{
    format::{read_dataformat, write_dataformat, DataFormat, DataFormatReader, DataHeaderOf},
    state::{AgentMode, AgentState, NodeState, PortConfig},
};

use crate::{
    server::jwt::Claims,
    state::{Agent, AgentAddrs, AgentFlags},
};

#[derive(Debug, Clone)]
pub struct AgentFormatHeader {
    pub version: u8,
    pub addrs: DataHeaderOf<AgentAddrs>,
    pub node: DataHeaderOf<NodeState>,
    pub flags: DataHeaderOf<AgentFlags>,
    pub ports: DataHeaderOf<PortConfig>,
}

impl DataFormat for AgentFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        let mut written = 0;
        written += self.version.write_data(writer)?;
        written += self.addrs.write_data(writer)?;
        written += write_dataformat(writer, &self.node)?;
        written += self.flags.write_data(writer)?;
        written += self.ports.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
                "AgentFormatHeader",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        Ok(AgentFormatHeader {
            version: reader.read_data(&())?,
            addrs: reader.read_data(&())?,
            node: read_dataformat(reader)?,
            flags: reader.read_data(&())?,
            ports: reader.read_data(&())?,
        })
    }
}

impl DataFormat for Agent {
    type Header = AgentFormatHeader;
    const LATEST_HEADER: Self::Header = AgentFormatHeader {
        version: 1,
        addrs: AgentAddrs::LATEST_HEADER,
        node: NodeState::LATEST_HEADER,
        flags: AgentFlags::LATEST_HEADER,
        ports: PortConfig::LATEST_HEADER,
    };

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        let mut written = 0;

        written += self.id.write_data(writer)?;
        written += self.claims.nonce.write_data(writer)?;
        match &self.state {
            AgentState::Inventory => {
                written += 0u8.write_data(writer)?;
            }
            AgentState::Node(env_id, state) => {
                written += 1u8.write_data(writer)?;
                written += env_id.write_data(writer)?;
                written += state.write_data(writer)?;
            }
        }
        written += self.flags.write_data(writer)?;
        written += self.ports.write_data(writer)?;
        written += self.addrs.write_data(writer)?;

        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(snops_common::format::DataReadError::unsupported(
                "Agent",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        let id = reader.read_data(&())?;
        let nonce = reader.read_data(&())?;
        let state = match reader.read_data(&())? {
            0u8 => AgentState::Inventory,
            1u8 => {
                let env_id = reader.read_data(&())?;
                let state = reader.read_data(&header.node)?;
                AgentState::Node(env_id, state)
            }
            n => {
                return Err(snops_common::format::DataReadError::Custom(format!(
                    "invalid AgentState discriminant: {n}"
                )))
            }
        };
        let flags = reader.read_data(&header.flags)?;
        let ports = reader.read_data(&header.ports)?;
        let addrs = reader.read_data(&header.addrs)?;

        Ok(Agent::from_components(
            Claims { id, nonce },
            state,
            flags,
            ports,
            addrs,
        ))
    }
}

impl DataFormat for AgentFlags {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        let mut written = 0;
        written += u8::from(self.mode).write_data(writer)?;
        written += self.labels.write_data(writer)?;
        written += self.local_pk.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
                "AgentFlags",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        Ok(AgentFlags {
            mode: AgentMode::from(u8::read_data(reader, &())?),
            labels: reader.read_data(&())?,
            local_pk: reader.read_data(&())?,
        })
    }
}

impl DataFormat for AgentAddrs {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        Ok(self.external.write_data(writer)? + self.internal.write_data(writer)?)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
                "AgentAddrs",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        Ok(AgentAddrs {
            external: reader.read_data(&())?,
            internal: reader.read_data(&())?,
        })
    }
}

#[cfg(test)]
#[rustfmt::skip]
mod test {
    use snops_common::{format::{read_dataformat, write_dataformat, DataFormat, PackedUint}, state::{AgentMode, AgentState, HeightRequest, KeyState, NodeState, PortConfig}, INTERN};
    use crate::{persist::AgentFormatHeader, state::{Agent, AgentAddrs, AgentFlags}};
    use std::net::{IpAddr, Ipv4Addr};

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr, $b:expr) => {
            #[test]
            fn $name() -> Result<(), Box<dyn std::error::Error>>{
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

    case!(agent_flags_1,
        AgentFlags,
        AgentFlags {
            mode: AgentMode::from(0u8),
            labels: [INTERN.get_or_intern("hello")].into_iter().collect(),
            local_pk: true,
        },
        [
            AgentFlags::LATEST_HEADER.to_byte_vec()?,
            0u8.to_byte_vec()?,
            PackedUint(1).to_byte_vec()?,
            "hello".to_string().to_byte_vec()?,
            true.to_byte_vec()?,
        ].concat()
    );

    case!(agent_addrs_1,
        AgentAddrs,
        AgentAddrs {
            external: Some("1.2.3.4".parse()?),
            internal: vec!["127.0.0.1".parse()?],
        },
        [
            AgentAddrs::LATEST_HEADER.to_byte_vec()?,
            Some(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))).to_byte_vec()?,
            vec![IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))].to_byte_vec()?,
        ].concat()
    );

    case!(agent_addrs_2,
        AgentAddrs,
        AgentAddrs {
            external: None,
            internal: vec![],
        },
        [
            AgentAddrs::LATEST_HEADER.to_byte_vec()?,
            None::<IpAddr>.to_byte_vec()?,
            Vec::<IpAddr>::new().to_byte_vec()?,
        ].concat()
    );

    case!(agent_1,
        crate::state::Agent,
        crate::state::Agent::from_components(
            crate::server::jwt::Claims {
                id: "agent".parse()?,
                nonce: 2,
            },
            AgentState::Inventory,
            AgentFlags {
                mode: AgentMode::from(0u8),
                labels: [INTERN.get_or_intern("hello")].into_iter().collect(),
                local_pk: true,
            },
            Some(PortConfig { node: 0, bft: 1, rest: 2, metrics: 3 }),
            Some(AgentAddrs {
                external: Some("1.2.3.4".parse()?),
                internal: vec!["127.0.0.1".parse()?],
            }),
        ),
        [
            AgentFormatHeader::LATEST_HEADER.to_byte_vec()?,
            Agent::LATEST_HEADER.to_byte_vec()?,
            "agent".to_string().to_byte_vec()?,
            2u16.to_byte_vec()?,
            0u8.to_byte_vec()?, // inventory state
            AgentFlags {
                mode: AgentMode::from(0u8),
                labels: [INTERN.get_or_intern("hello")].into_iter().collect(),
                local_pk: true,
            }.to_byte_vec()?,
            Some(PortConfig { node: 0, bft: 1, rest: 2, metrics: 3 }).to_byte_vec()?,
            Some(AgentAddrs {
                external: Some("1.2.3.4".parse()?),
                internal: vec!["127.0.0.1".parse()?],
            }).to_byte_vec()?,
        ].concat()
    );

    case!(agent_2,
        crate::state::Agent,
        crate::state::Agent::from_components(
            crate::server::jwt::Claims {
                id: "agent".parse()?,
                nonce: 2,
            },
            AgentState::Node("env".parse()?, Box::new(NodeState {
                node_key: "client/foo".parse()?,
                private_key: KeyState::None,
                height: (0, HeightRequest::Top),
                online: true,
                peers: vec![],
                validators: vec![],
                env: Default::default(),
            })),
            AgentFlags {
                mode: AgentMode::from(5u8),
                labels: Default::default(),
                local_pk: true,
            },
            Some(PortConfig { node: 3, bft: 2, rest: 1, metrics: 0 }),
            Some(AgentAddrs {
                external: None,
                internal: vec![],
            }),
        ),
        [
            AgentFormatHeader::LATEST_HEADER.to_byte_vec()?,
            Agent::LATEST_HEADER.to_byte_vec()?,
            "agent".to_string().to_byte_vec()?,
            2u16.to_byte_vec()?,
            1u8.to_byte_vec()?, // node state
            "env".to_string().to_byte_vec()?,
            NodeState {
                node_key: "client/foo".parse()?,
                private_key: KeyState::None,
                height: (0, HeightRequest::Top),
                online: true,
                peers: vec![],
                validators: vec![],
                env: Default::default(),
            }.to_byte_vec()?,
            AgentFlags {
                mode: AgentMode::from(5u8),
                labels: Default::default(),
                local_pk: true,
            }.to_byte_vec()?,
            Some(PortConfig { node: 3, bft: 2, rest: 1, metrics: 0 }).to_byte_vec()?,
            Some(AgentAddrs {
                external: None,
                internal: vec![],
            }).to_byte_vec()?,
        ].concat()
    );
}
