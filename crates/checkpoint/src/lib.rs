use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod retention_tests;

mod errors;
mod header;
mod manager;
mod retention;

pub use errors::*;
pub use header::*;
pub use manager::*;
pub use retention::*;

#[cfg(feature = "write")]
mod checkpoint;
#[cfg(feature = "write")]
mod content;
#[cfg(feature = "write")]
mod ledger;
#[cfg(feature = "write")]
pub(crate) mod snarkos;
#[cfg(feature = "write")]
pub use checkpoint::*;
#[cfg(feature = "write")]
pub use content::*;

pub fn path_from_height<D: Display>(path: &Path, height: D) -> Option<PathBuf> {
    path.parent()
        .map(|p| p.join(format!("{height}.checkpoint")))
}
