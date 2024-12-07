use std::{collections::HashSet, fs, path::PathBuf};

use anyhow::{ensure, Result};
use bech32::ToBase32;
use clap::Parser;
use colored::Colorize;
use indexmap::IndexMap;
use rand::SeedableRng;
use rand_chacha::ChaChaRng;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use snarkvm::{console::program::Network, prelude::Itertools, utilities::ToBytes};

use crate::{Address, PrivateKey};

/// Given a seed and a count, generate a number of accounts.
#[derive(Debug, Clone, Parser)]
pub struct GenAccounts {
    /// Number of accounts to generate
    #[clap(default_value_t = 1)]
    pub count: u16,

    /// Vanity prefix for addresses
    #[clap(short, long)]
    pub vanity: Option<String>,

    /// Where to write the output to
    #[clap(short, long)]
    pub output: Option<PathBuf>,

    /// The seed to use when generating private keys
    /// If unpassed or used with --vanity, uses a random seed
    #[clap(name = "seed", short, long)]
    pub seed: Option<u64>,
}

pub const BECH32M_CHARSET: &str = "0123456789acdefghjklmnpqrstuvwxyz";

#[derive(Clone, Copy)]
struct VanityCheck<'a>(&'a [bech32::u5]);

impl bech32::WriteBase32 for VanityCheck<'_> {
    type Err = bool;

    fn write_u5(&mut self, data: bech32::u5) -> std::result::Result<(), Self::Err> {
        // vanity was found
        if self.0.is_empty() {
            return Err(true);
        }

        // newest u5 is invalid
        if data != self.0[0] {
            return Err(false);
        }

        // remove the u5 from the vanity prefix
        self.0 = &self.0[1..];

        Ok(())
    }
}

impl GenAccounts {
    pub fn parse<N: Network>(self) -> Result<()> {
        let mut rng = self
            .seed
            .map(ChaChaRng::seed_from_u64)
            .unwrap_or_else(ChaChaRng::from_entropy);

        let vanity = self.vanity.as_ref().map(|vanity| {
            let illegal_chars = vanity
                .chars()
                .filter(|c| !BECH32M_CHARSET.contains(*c))
                .collect::<HashSet<_>>();
            ensure!(
                illegal_chars.is_empty(),
                "Vanity string contains invalid characters: `{}`. Only the following characters are allowed: {BECH32M_CHARSET}",
                illegal_chars.iter().join("")
            );

            let (_, prefix) = bech32::decode_without_checksum(&format!("aleo1{vanity}"))?;
            Ok(prefix)
        }).transpose()?;

        // Add additional accounts to the public balances
        let accounts: IndexMap<Address<N>, PrivateKey<N>> = (0..self.count)
            .map(|_| {
                if let Some(vanity) = &vanity {
                    loop {
                        let found_vanity = (0..65536).into_par_iter().find_map_any(|_| {
                            let key = PrivateKey::new(&mut ChaChaRng::from_entropy()).unwrap();
                            let addr = Address::try_from(&key).unwrap();
                            let has_vanity = Err(true)
                                == ToBytes::to_bytes_le(&addr)
                                    .unwrap()
                                    .write_base32(&mut VanityCheck(vanity));
                            has_vanity.then_some((addr, key))
                        });
                        if let Some((addr, key)) = found_vanity {
                            break Ok((addr, key));
                        } else {
                            continue;
                        }
                    }
                } else {
                    let key = PrivateKey::new(&mut rng)?;
                    let addr = Address::try_from(&key)?;
                    Ok((addr, key))
                }
            })
            .collect::<Result<IndexMap<_, _>>>()?;

        match self.output {
            // Write the accounts JSON file.
            Some(accounts_file) => {
                let file = fs::File::options()
                    .append(false)
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(&accounts_file)?;
                serde_json::to_writer_pretty(file, &accounts)?;

                println!(
                    "Accounts written to {}.",
                    accounts_file.display().to_string().yellow()
                );
            }

            // Write the accounts to stdout if no file was passed.
            None => {
                println!("Generated {} accounts:", self.count,);
                for (addr, key) in accounts {
                    println!(
                        "\t{}: {}",
                        addr.to_string().yellow(),
                        key.to_string().cyan()
                    );
                }
            }
        }

        Ok(())
    }
}
