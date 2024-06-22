use std::{path::PathBuf, str::FromStr};

use anyhow::{bail, Result};
use clap::Args;

use crate::{Network, PrivateKey};

/// A command line argument for specifying the account private key of the node.
/// Done by a private key or a private key file.
#[derive(Debug, Args, Clone)]
#[group(required = true, multiple = false)]
pub struct Key<N: Network> {
    /// Specify the account private key of the node
    #[clap(long = "private-key")]
    pub private_key: Option<PrivateKey<N>>,
    /// Specify the account private key of the node
    #[clap(long = "private-key-file")]
    pub private_key_file: Option<PathBuf>,
}

impl<N: Network> Key<N> {
    pub fn try_get(self) -> Result<PrivateKey<N>> {
        match (self.private_key, self.private_key_file) {
            (Some(key), None) => Ok(key),
            (None, Some(file)) => {
                let raw = std::fs::read_to_string(file)?.trim().to_string();
                Ok(PrivateKey::from_str(&raw)?)
            }
            // clap should make this unreachable, but serde might not
            _ => bail!("Either `private-key` or `private-key-file` must be set"),
        }
    }
}
