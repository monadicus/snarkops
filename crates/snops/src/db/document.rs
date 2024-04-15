use std::str::FromStr;

use bytes::{Buf, BufMut};
use sled::IVec;
use snops_common::state::InternedId;

use super::{error::DatabaseError, Database};

pub trait DbDocument: Sized {
    type Key;

    /// Load the state of the object from the database at load time
    fn restore(db: &Database, key: Self::Key) -> Result<Option<Self>, DatabaseError>;

    /// Save the state of the object to the database (idempotent)
    fn save(&self, db: &Database, key: Self::Key) -> Result<(), DatabaseError>;

    /// Delete the state of the object to the database (idempotent)
    fn delete(db: &Database, key: Self::Key) -> Result<bool, DatabaseError>;

    // Find all keys that are dangling (referenced by another object that does
    // not exist)
    // fn dangling(&self, db: &Database) -> Result<impl Iterator<Item = Self::Key>,
    // DatabaseError>;
}

pub trait DbCollection: Sized {
    /// Restore the state of a collection from the database at load time
    fn restore(db: &Database) -> Result<Self, DatabaseError>;
}

/// Load an interned id from a row in the database, returning None if there is
/// some parsing error.
pub fn load_interned_id(row: Result<(IVec, IVec), sled::Error>, kind: &str) -> Option<InternedId> {
    match row {
        Ok((key_bytes, _)) => {
            let key_str = match std::str::from_utf8(&key_bytes) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Error reading {kind} key from store: {e}");
                    return None;
                }
            };

            match InternedId::from_str(key_str) {
                Ok(key_id) => Some(key_id),
                Err(e) => {
                    tracing::error!("Error parsing {kind} key from store: {e}");
                    None
                }
            }
        }
        Err(e) => {
            tracing::error!("Error reading {kind} row from store: {e}");
            None
        }
    }
}

pub fn concat_ids<const S: usize>(ids: [InternedId; S]) -> Vec<u8> {
    let mut buf = Vec::new();
    for id in ids {
        buf.extend_from_slice(id.as_ref());
    }
    buf
}

/// Bincode does not support deserialize_any so many of our serializers used for
/// parsing yaml are not supported.
pub trait BEncDec: Sized {
    fn as_bytes(&self) -> bincode::Result<Vec<u8>>;
    fn from_bytes(bytes: &[u8]) -> bincode::Result<Self>;

    fn read_bytes(buf: &mut &[u8]) -> bincode::Result<Self> {
        if buf.remaining() < 4 {
            return Err(bincode::ErrorKind::Custom(
                "Buffer too short to read length prefix".to_owned(),
            )
            .into());
        }
        let len = buf.get_u32() as usize;
        if buf.remaining() < len {
            return Err(bincode::ErrorKind::Custom(
                "Buffer too short to read expected length".to_owned(),
            )
            .into());
        }
        let res = Self::from_bytes(&buf[..len])?;
        buf.advance(len);
        Ok(res)
    }

    fn write_bytes(&self, buf: &mut Vec<u8>) -> bincode::Result<()> {
        let bytes = self.as_bytes()?;
        buf.put_u32(bytes.len() as u32);
        buf.extend_from_slice(&bytes);
        Ok(())
    }
}

#[macro_export]
macro_rules! impl_bencdec_serde {
    ($name:ident) => {
        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_bytes(&self.as_bytes().unwrap())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<$name, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let bytes = Vec::<u8>::deserialize(deserializer)?;
                BEncDec::from_bytes(&bytes).map_err(serde::de::Error::custom)
            }
        }
    };
}
