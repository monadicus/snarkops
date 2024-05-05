use snops_common::{
    format::{read_dataformat, write_dataformat, DataFormat, DataFormatReader},
    state::{AgentMode, AgentState, NodeState, PortConfig},
};

use crate::{
    server::jwt::Claims,
    state::{Agent, AgentAddrs, AgentFlags},
};

#[derive(Debug, Clone)]
pub struct AgentFormatHeader {
    pub version: u8,
    pub addrs: <AgentAddrs as DataFormat>::Header,
    pub node: <NodeState as DataFormat>::Header,
    pub flags: <AgentFlags as DataFormat>::Header,
    pub ports: <PortConfig as DataFormat>::Header,
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
        addrs: <AgentAddrs as DataFormat>::LATEST_HEADER,
        node: <NodeState as DataFormat>::LATEST_HEADER,
        flags: <AgentFlags as DataFormat>::LATEST_HEADER,
        ports: <PortConfig as DataFormat>::LATEST_HEADER,
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
