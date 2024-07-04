use std::path::PathBuf;

use self::error::DatabaseError;

pub mod error;
pub mod tree;

pub trait Database: Sized {
    fn open(path: &PathBuf) -> Result<Self, DatabaseError>;
}
