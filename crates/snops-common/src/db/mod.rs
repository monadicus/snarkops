use std::path::Path;

use self::error::DatabaseError;

pub mod error;
pub mod tree;

pub trait Database: Sized {
    fn open<P: AsRef<Path>>(path: P) -> Result<Self, DatabaseError>;
}
