use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use snops_checkpoint::RetentionPolicy;

use crate::{
    binaries::BinaryEntry,
    format::{DataFormat, DataHeaderOf},
    prelude::StorageId,
    state::{InternedId, LatestBlockInfo, NetworkId},
};

/// Metadata about a checkpoint file
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CheckpointMeta {
    pub height: u32,
    pub timestamp: i64,
    pub filename: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnvInfo {
    pub network: NetworkId,
    pub storage: StorageInfo,
    pub block: Option<LatestBlockInfo>,
}

/// Lighter-weight version of EnvInfo for the agent
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentEnvInfo {
    pub network: NetworkId,
    pub storage: StorageInfo,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct StorageInfo {
    /// String id of this storage
    pub id: StorageId,
    /// The retention policy used for this storage
    pub retention_policy: Option<RetentionPolicy>,
    /// Whether to persist the ledger
    pub persist: bool,
    /// Version identifier for this ledger
    pub version: u16,
    /// Whether to use the network's native genesis block
    pub native_genesis: bool,
    /// A map of the snarkos binary ids to a potential download url (when None,
    /// download from the control plane)
    pub binaries: IndexMap<InternedId, BinaryEntry>,
}

#[derive(Debug, Clone)]
pub struct EnvInfoHeader {
    pub version: u8,
    pub network: DataHeaderOf<NetworkId>,
    pub storage: DataHeaderOf<StorageInfo>,
    pub block: DataHeaderOf<LatestBlockInfo>,
}

impl DataFormat for EnvInfoHeader {
    type Header = (u8, DataHeaderOf<DataHeaderOf<StorageInfo>>);
    const LATEST_HEADER: Self::Header = (1, DataHeaderOf::<StorageInfo>::LATEST_HEADER);

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        let mut written = self.version.write_data(writer)?;
        written += self.network.write_data(writer)?;
        written += self.storage.write_data(writer)?;
        written += self.block.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if header.0 != Self::LATEST_HEADER.0 {
            return Err(crate::format::DataReadError::unsupported(
                "EnvInfoHeader",
                Self::LATEST_HEADER.0,
                header.0,
            ));
        }
        Ok(Self {
            version: u8::read_data(reader, &())?,
            network: DataHeaderOf::<NetworkId>::read_data(reader, &())?,
            storage: DataHeaderOf::<StorageInfo>::read_data(reader, &header.1)?,
            block: DataHeaderOf::<LatestBlockInfo>::read_data(reader, &())?,
        })
    }
}

impl DataFormat for EnvInfo {
    type Header = EnvInfoHeader;
    const LATEST_HEADER: Self::Header = EnvInfoHeader {
        version: 1,
        network: NetworkId::LATEST_HEADER,
        storage: StorageInfo::LATEST_HEADER,
        block: LatestBlockInfo::LATEST_HEADER,
    };

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        let mut written = self.network.write_data(writer)?;
        written += self.storage.write_data(writer)?;
        written += self.block.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if header.version != 1 {
            return Err(crate::format::DataReadError::unsupported(
                "EnvInfo",
                1,
                header.version,
            ));
        }
        Ok(Self {
            network: NetworkId::read_data(reader, &header.network)?,
            storage: StorageInfo::read_data(reader, &header.storage)?,
            block: Option::<LatestBlockInfo>::read_data(reader, &header.block)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AgentEnvInfoHeader {
    pub version: u8,
    pub network: DataHeaderOf<NetworkId>,
    pub storage: DataHeaderOf<StorageInfo>,
}

impl DataFormat for AgentEnvInfoHeader {
    type Header = (u8, DataHeaderOf<DataHeaderOf<StorageInfo>>);
    const LATEST_HEADER: Self::Header = (1, DataHeaderOf::<StorageInfo>::LATEST_HEADER);

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        let mut written = self.version.write_data(writer)?;
        written += self.network.write_data(writer)?;
        written += self.storage.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if header.0 != Self::LATEST_HEADER.0 {
            return Err(crate::format::DataReadError::unsupported(
                "EnvInfoHeader",
                Self::LATEST_HEADER.0,
                header.0,
            ));
        }
        Ok(Self {
            version: u8::read_data(reader, &())?,
            network: DataHeaderOf::<NetworkId>::read_data(reader, &())?,
            storage: DataHeaderOf::<StorageInfo>::read_data(reader, &header.1)?,
        })
    }
}

impl DataFormat for AgentEnvInfo {
    type Header = AgentEnvInfoHeader;
    const LATEST_HEADER: Self::Header = AgentEnvInfoHeader {
        version: 1,
        network: NetworkId::LATEST_HEADER,
        storage: StorageInfo::LATEST_HEADER,
    };

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        let mut written = self.network.write_data(writer)?;
        written += self.storage.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if header.version != 1 {
            return Err(crate::format::DataReadError::unsupported(
                "EnvInfo",
                1,
                header.version,
            ));
        }
        Ok(Self {
            network: NetworkId::read_data(reader, &header.network)?,
            storage: StorageInfo::read_data(reader, &header.storage)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct StorageInfoHeader {
    pub version: u8,
    pub retention_policy: DataHeaderOf<RetentionPolicy>,
    pub binaries: DataHeaderOf<BinaryEntry>,
}

impl DataFormat for StorageInfoHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        let mut written = self.version.write_data(writer)?;
        written += self.retention_policy.write_data(writer)?;
        written += self.binaries.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(crate::format::DataReadError::unsupported(
                "StorageInfoHeader",
                Self::LATEST_HEADER,
                header,
            ));
        }
        Ok(Self {
            version: u8::read_data(reader, &())?,
            retention_policy: DataHeaderOf::<RetentionPolicy>::read_data(reader, &((), ()))?,
            binaries: DataHeaderOf::<BinaryEntry>::read_data(reader, &())?,
        })
    }
}

impl DataFormat for StorageInfo {
    type Header = StorageInfoHeader;

    const LATEST_HEADER: Self::Header = StorageInfoHeader {
        version: 2,
        retention_policy: RetentionPolicy::LATEST_HEADER,
        binaries: BinaryEntry::LATEST_HEADER,
    };

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        let mut written = self.id.write_data(writer)?;
        written += self.retention_policy.write_data(writer)?;
        written += self.persist.write_data(writer)?;
        written += self.version.write_data(writer)?;
        written += self.native_genesis.write_data(writer)?;
        written += self.binaries.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if header.version == 0 || header.version > Self::LATEST_HEADER.version {
            return Err(crate::format::DataReadError::unsupported(
                "StorageInfo",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        let id = StorageId::read_data(reader, &())?;
        let retention_policy =
            Option::<RetentionPolicy>::read_data(reader, &header.retention_policy)?;

        // Omit checkpoints from a previous version
        if header.version == 1 {
            Vec::<(u32, i64, String)>::read_data(reader, &((), (), ()))?;
        };

        let persist = bool::read_data(reader, &())?;
        let version = u16::read_data(reader, &())?;
        let native_genesis = bool::read_data(reader, &())?;
        let binaries =
            IndexMap::<InternedId, BinaryEntry>::read_data(reader, &((), header.binaries))?;
        Ok(Self {
            id,
            retention_policy,
            persist,
            version,
            native_genesis,
            binaries,
        })
    }
}
