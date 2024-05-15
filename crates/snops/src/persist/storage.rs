use checkpoint::{CheckpointManager, RetentionPolicy};
use snops_common::{
    constant::LEDGER_BASE_DIR,
    state::{InternedId, StorageId},
};
use tracing::info;

use super::prelude::*;
use crate::{
    cli::Cli,
    schema::{
        error::StorageError,
        storage::{pick_commitee_addr, read_to_addrs, LoadedStorage, STORAGE_DIR},
    },
};

/// Metadata for storage that can be used to restore a loaded storage
pub struct PersistStorage {
    pub id: StorageId,
    pub version: u16,
    pub persist: bool,
    pub accounts: Vec<InternedId>,
    pub retention_policy: Option<RetentionPolicy>,
}

#[derive(Debug, Clone)]
pub struct PersistStorageFormatHeader {
    pub version: u8,
    pub retention_policy: DataHeaderOf<RetentionPolicy>,
}

impl DataFormat for PersistStorageFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        Ok(self.version.write_data(writer)? + self.retention_policy.write_data(writer)?)
    }

    fn read_data<R: Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
                "PersistStorageFormatHeader",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        Ok(PersistStorageFormatHeader {
            version: reader.read_data(&())?,
            retention_policy: reader.read_data(&((), ()))?,
        })
    }
}

impl From<&LoadedStorage> for PersistStorage {
    fn from(storage: &LoadedStorage) -> Self {
        Self {
            id: storage.id,
            version: storage.version,
            persist: storage.persist,
            accounts: storage.accounts.keys().cloned().collect(),
            retention_policy: storage.checkpoints.as_ref().map(|c| c.policy().clone()),
        }
    }
}

impl PersistStorage {
    pub async fn load(self, cli: &Cli) -> Result<LoadedStorage, StorageError> {
        let id = self.id;
        let mut storage_path = cli.path.join(STORAGE_DIR);
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

        Ok(LoadedStorage {
            id,
            version: self.version,
            persist: self.persist,
            committee: read_to_addrs(pick_commitee_addr, &committee_file).await?,
            checkpoints,
            // TODO: waiting for #116 to be merged, then make a reusable function
            accounts: Default::default(),
        })
    }
}

impl DataFormat for PersistStorage {
    type Header = PersistStorageFormatHeader;
    const LATEST_HEADER: Self::Header = PersistStorageFormatHeader {
        version: 1,
        retention_policy: RetentionPolicy::LATEST_HEADER,
    };

    fn write_data<W: Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        let mut written = 0;

        written += self.id.write_data(writer)?;
        written += self.version.write_data(writer)?;
        written += self.persist.write_data(writer)?;
        written += self.accounts.write_data(writer)?;
        written += self.retention_policy.write_data(writer)?;

        Ok(written)
    }

    fn read_data<R: Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(snops_common::format::DataReadError::unsupported(
                "PersistStorage",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        Ok(PersistStorage {
            id: reader.read_data(&())?,
            version: reader.read_data(&())?,
            persist: reader.read_data(&())?,
            accounts: reader.read_data(&())?,
            retention_policy: reader.read_data(&header.retention_policy)?,
        })
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use checkpoint::RetentionPolicy;
    use snops_common::{
        format::{read_dataformat, write_dataformat, DataFormat},
        state::InternedId,
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
        ]
        .concat()
    );

    case!(
        storage,
        PersistStorage,
        PersistStorage {
            id: InternedId::from_str("id")?,
            version: 1,
            persist: true,
            accounts: vec![],
            retention_policy: None,
        },
        [
            PersistStorageFormatHeader::LATEST_HEADER.to_byte_vec()?,
            PersistStorage::LATEST_HEADER.to_byte_vec()?,
            InternedId::from_str("id")?.to_byte_vec()?,
            1u16.to_byte_vec()?,
            true.to_byte_vec()?,
            Vec::<InternedId>::new().to_byte_vec()?,
            None::<RetentionPolicy>.to_byte_vec()?,
        ]
        .concat()
    );

    case!(
        storage_base,
        PersistStorage,
        PersistStorage {
            id: InternedId::from_str("base")?,
            version: 0,
            persist: false,
            accounts: vec![InternedId::from_str("accounts")?],
            retention_policy: None,
        },
        [
            1, 1, 1, 1, 1, 4, 98, 97, 115, 101, 0, 0, 0, 1, 1, 1, 8, 97, 99, 99, 111, 117, 110,
            116, 115, 0
        ]
    );
}
