use snops_common::{
    api::EnvInfo,
    format::{DataFormat, DataHeaderOf},
    state::{NetworkId, StorageId},
};

pub struct EnvState {
    network_id: NetworkId,
    storage_id: StorageId,
    storage_version: u16,
}

impl EnvState {
    pub fn changed(&self, env_info: &EnvInfo) -> bool {
        env_info.storage.version != self.storage_version
            || env_info.storage.id != self.storage_id
            || env_info.network != self.network_id
    }
}

impl From<&EnvInfo> for EnvState {
    fn from(info: &EnvInfo) -> Self {
        Self {
            network_id: info.network,
            storage_id: info.storage.id,
            storage_version: info.storage.version,
        }
    }
}

impl Default for EnvState {
    fn default() -> Self {
        Self {
            network_id: NetworkId::Mainnet,
            storage_id: StorageId::default(),
            storage_version: 0,
        }
    }
}

impl DataFormat for EnvState {
    type Header = (u8, DataHeaderOf<NetworkId>);

    const LATEST_HEADER: Self::Header = (1u8, NetworkId::LATEST_HEADER);

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        Ok(self.network_id.write_data(writer)?
            + self.storage_id.write_data(writer)?
            + self.storage_version.write_data(writer)?)
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if header.0 != Self::LATEST_HEADER.0 {
            return Err(snops_common::format::DataReadError::unsupported(
                "EnvIdentifier",
                Self::LATEST_HEADER.0,
                header.0,
            ));
        }

        Ok(Self {
            network_id: NetworkId::read_data(reader, &header.1)?,
            storage_id: StorageId::read_data(reader, &())?,
            storage_version: u16::read_data(reader, &())?,
        })
    }
}
