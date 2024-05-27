use std::{fs, path::PathBuf};

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use indexmap::IndexMap;
use rand::SeedableRng;
use rand_chacha::ChaChaRng;
use snarkvm::console::program::Network;

use crate::{Address, PrivateKey};

#[derive(Debug, Clone, Parser)]
pub struct GenAccounts {
    /// Number of accounts to generate
    pub count: u16,

    /// Where to write the output to
    #[clap(short, long)]
    pub output: Option<PathBuf>,

    /// The seed to use when generating private keys
    /// If unpassed, uses a random seed
    #[clap(name = "seed", short, long)]
    pub seed: Option<u64>,
}

impl GenAccounts {
    pub fn parse<N: Network>(self) -> Result<()> {
        let mut rng = self
            .seed
            .map(ChaChaRng::seed_from_u64)
            .unwrap_or_else(ChaChaRng::from_entropy);

        // Add additional accounts to the public balances
        let accounts: IndexMap<Address<N>, PrivateKey<N>> = (0..self.count)
            .map(|_| {
                let key = PrivateKey::new(&mut rng)?;
                let addr = Address::try_from(&key)?;
                Ok((addr, key))
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
