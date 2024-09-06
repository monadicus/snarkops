use checkpoint::{CheckpointManager, RetentionPolicy};
use indexmap::IndexMap;
use snops_common::{
    binaries::BinaryEntry,
    constant::LEDGER_BASE_DIR,
    key_source::ACCOUNTS_KEY_ID,
    state::{InternedId, NetworkId, StorageId},
};
use tracing::{info, warn};

use super::prelude::*;
use crate::{
    cli::Config,
    schema::{
        error::StorageError,
        storage::{
            pick_account_addr, pick_additional_addr, pick_commitee_addr, read_to_addrs,
            LoadedStorage, STORAGE_DIR,
        },
    },
};

/// Metadata for storage that can be used to restore a loaded storage
pub struct PersistStorage {
    pub id: StorageId,
    pub network: NetworkId,
    pub version: u16,
    pub persist: bool,
    pub accounts: Vec<InternedId>,
    pub retention_policy: Option<RetentionPolicy>,
    pub native_genesis: bool,
    pub binaries: IndexMap<InternedId, BinaryEntry>,
}

#[derive(Debug, Clone)]
pub struct PersistStorageFormatHeader {
    pub version: u8,
    pub retention_policy: DataHeaderOf<RetentionPolicy>,
    pub network: DataHeaderOf<NetworkId>,
    pub binaries: DataHeaderOf<BinaryEntry>,
}

impl DataFormat for PersistStorageFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 3;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(self.version.write_data(writer)?
            + self.retention_policy.write_data(writer)?
            + self.network.write_data(writer)?
            + self.binaries.write_data(writer)?)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header > Self::LATEST_HEADER || *header < 2 {
            return Err(DataReadError::unsupported(
                "PersistStorageFormatHeader",
                format!("1, 2, or {}", Self::LATEST_HEADER),
                *header,
            ));
        }

        Ok(PersistStorageFormatHeader {
            version: reader.read_data(&())?,
            retention_policy: reader.read_data(&((), ()))?,
            network: if *header >= 2 {
                reader.read_data(&())?
            } else {
                0
            },
            binaries: if *header >= 3 {
                reader.read_data(&())?
            } else {
                0
            },
        })
    }
}

impl From<&LoadedStorage> for PersistStorage {
    fn from(storage: &LoadedStorage) -> Self {
        Self {
            id: storage.id,
            network: storage.network,
            version: storage.version,
            persist: storage.persist,
            accounts: storage.accounts.keys().cloned().collect(),
            retention_policy: storage.checkpoints.as_ref().map(|c| c.policy().clone()),
            native_genesis: storage.native_genesis,
            binaries: storage.binaries.clone(),
        }
    }
}

impl PersistStorage {
    pub async fn load(self, config: &Config) -> Result<LoadedStorage, StorageError> {
        let id = self.id;
        let mut storage_path = config.path.join(STORAGE_DIR);
        storage_path.push(self.network.to_string());
        storage_path.push(id.to_string());
        let committee_file = storage_path.join("committee.json");

        let checkpoints = self
            .retention_policy
            .map(|policy| {
                CheckpointManager::load(storage_path.join(LEDGER_BASE_DIR), policy)
                    .map_err(StorageError::CheckpointManager)
            })
            .transpose()?;

        if let Some(checkpoints) = &checkpoints {
            info!("storage {id} checkpoint manager loaded {checkpoints}");
        } else {
            info!("storage {id} loaded without a checkpoint manager");
        }

        let mut accounts = IndexMap::new();

        // load accounts json
        for name in &self.accounts {
            let path = storage_path.join(&format!("{name}.json"));

            let res = if *name == *ACCOUNTS_KEY_ID {
                read_to_addrs(pick_additional_addr, &path).await
            } else {
                read_to_addrs(pick_account_addr, &path).await
            };

            match res {
                Ok(account) => {
                    accounts.insert(*name, account);
                }
                Err(e) => {
                    warn!("storage {id} failed to load account file {name}: {e}")
                }
            }
        }

        Ok(LoadedStorage {
            id,
            network: self.network,
            version: self.version,
            persist: self.persist,
            committee: read_to_addrs(pick_commitee_addr, &committee_file).await?,
            checkpoints,
            native_genesis: self.native_genesis,
            accounts,
            binaries: self.binaries,
        })
    }
}

impl DataFormat for PersistStorage {
    type Header = PersistStorageFormatHeader;
    const LATEST_HEADER: Self::Header = PersistStorageFormatHeader {
        version: 1,
        retention_policy: RetentionPolicy::LATEST_HEADER,
        network: NetworkId::LATEST_HEADER,
        binaries: BinaryEntry::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;

        written += self.id.write_data(writer)?;
        written += self.network.write_data(writer)?;
        written += self.version.write_data(writer)?;
        written += self.persist.write_data(writer)?;
        written += self.accounts.write_data(writer)?;
        written += self.retention_policy.write_data(writer)?;
        written += self.native_genesis.write_data(writer)?;
        written += self.binaries.write_data(writer)?;

        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if header.version == 0 || header.version > Self::LATEST_HEADER.version {
            return Err(DataReadError::unsupported(
                "PersistStorage",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        Ok(PersistStorage {
            id: reader.read_data(&())?,
            network: if header.network > 0 {
                reader.read_data(&header.network)?
            } else {
                Default::default()
            },
            version: reader.read_data(&())?,
            persist: reader.read_data(&())?,
            accounts: reader.read_data(&())?,
            retention_policy: reader.read_data(&header.retention_policy)?,
            native_genesis: reader.read_data(&())?,
            binaries: if header.binaries > 0 {
                reader.read_data(&((), header.binaries))?
            } else {
                IndexMap::new()
            },
        })
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use checkpoint::RetentionPolicy;
    use indexmap::IndexMap;
    use snops_common::{
        binaries::BinaryEntry,
        format::{read_dataformat, write_dataformat, DataFormat},
        state::{InternedId, NetworkId},
    };

    use crate::persist::{PersistStorage, PersistStorageFormatHeader};

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
        storage_header,
        PersistStorageFormatHeader,
        PersistStorage::LATEST_HEADER,
        [
            PersistStorageFormatHeader::LATEST_HEADER.to_byte_vec()?,
            PersistStorage::LATEST_HEADER.version.to_byte_vec()?,
            RetentionPolicy::LATEST_HEADER.to_byte_vec()?,
            NetworkId::LATEST_HEADER.to_byte_vec()?,
            BinaryEntry::LATEST_HEADER.to_byte_vec()?,
        ]
        .concat()
    );

    case!(
        storage,
        PersistStorage,
        PersistStorage {
            id: InternedId::from_str("id")?,
            network: NetworkId::default(),
            version: 1,
            persist: true,
            accounts: vec![],
            retention_policy: None,
            native_genesis: false,
            binaries: IndexMap::new(),
        },
        [
            PersistStorageFormatHeader::LATEST_HEADER.to_byte_vec()?,
            PersistStorage::LATEST_HEADER.to_byte_vec()?,
            InternedId::from_str("id")?.to_byte_vec()?,
            NetworkId::default().to_byte_vec()?,
            1u16.to_byte_vec()?,
            true.to_byte_vec()?,
            Vec::<InternedId>::new().to_byte_vec()?,
            None::<RetentionPolicy>.to_byte_vec()?,
            false.to_byte_vec()?,
            IndexMap::<InternedId, BinaryEntry>::new().to_byte_vec()?,
        ]
        .concat()
    );

    case!(
        storage_base,
        PersistStorage,
        PersistStorage {
            id: InternedId::from_str("base")?,
            network: NetworkId::default(),
            version: 0,
            persist: false,
            accounts: vec![InternedId::from_str("accounts")?],
            retention_policy: None,
            native_genesis: true,
            binaries: IndexMap::new(),
        },
        [
            3, 1, 1, 1, 1, 1, 1, 4, 98, 97, 115, 101, 0, 0, 0, 0, 1, 1, 1, 8, 97, 99, 99, 111, 117,
            110, 116, 115, 0, 1, 0
        ]
    );
}
