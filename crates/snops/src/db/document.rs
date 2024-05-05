use std::str::FromStr;

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
