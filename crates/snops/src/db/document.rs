use super::{error::DatabaseError, Database};

pub trait DbDocument: Sized {
    type Key;

    /// Load the state of the object from the database at load time
    fn restore(db: &Database, key: Self::Key) -> Result<Option<Self>, DatabaseError>;

    /// Save the state of the object to the database (idempotent)
    fn save(&self, db: &Database, key: Self::Key) -> Result<(), DatabaseError>;

    /// Delete the state of the object to the database (idempotent)
    fn delete(&self, db: &Database, key: Self::Key) -> Result<bool, DatabaseError>;

    // Find all keys that are dangling (referenced by another object that does
    // not exist)
    // fn dangling(&self, db: &Database) -> Result<impl Iterator<Item = Self::Key>,
    // DatabaseError>;
}

pub trait DbCollection: Sized {
    /// Restore the state of a collection from the database at load time
    fn restore(db: &Database) -> Result<Self, DatabaseError>;
}
