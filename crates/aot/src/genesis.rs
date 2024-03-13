// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkOS library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{fs, path::PathBuf, str::FromStr};

use aleo_std::StorageMode;
use anyhow::{ensure, Result};
use clap::Parser;
use colored::Colorize;
use indexmap::IndexMap;
use rand::SeedableRng;
use rand_chacha::ChaChaRng;
use serde::{Deserialize, Serialize};
use snarkos_cli::commands::{load_or_compute_genesis, DEVELOPMENT_MODE_RNG_SEED};
use snarkvm::{
    console::{account::PrivateKey, network::MainnetV0, program::Network, types::Address},
    ledger::{
        committee::{Committee, MIN_VALIDATOR_STAKE},
        store::helpers::rocksdb::ConsensusDB,
        Ledger,
    },
    utilities::ToBytes,
};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Balances(IndexMap<Address<MainnetV0>, u64>);
impl FromStr for Balances {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Debug, Parser)]
pub struct Genesis {
    /// The private key to use when generating the genesis block. Generates one randomly if not passed.
    #[clap(name = "genesis-key", short, long)]
    genesis_key: Option<PrivateKey<MainnetV0>>,
    /// Where to write the genesis block to.
    #[clap(name = "output", short, long, default_value = "genesis.block")]
    output: PathBuf,
    /// The committee size. Not used if --bonded-balances is set.
    #[clap(name = "committee-size", long, default_value_t = 4)]
    committee_size: u16,
    /// Additional number of accounts that aren't validators to add balances to.
    #[clap(name = "additional-accounts", long, default_value_t = 0)]
    additional_accounts: u16,
    /// The balance to add to the number of accounts specified by additional-accounts.
    #[clap(
        name = "additional-accounts-balance",
        long,
        default_value_t = 100_000_000
    )]
    additional_accounts_balance: u64,
    /// A place to write out the additionally generated accounts by --additional-accounts.
    #[clap(name = "additional-accounts-file", long)]
    additional_accounts_file: Option<PathBuf>,
    /// The seed to use when generating committee private keys and the genesis block. If unpassed, uses DEVELOPMENT_MODE_RNG_SEED.
    #[clap(name = "seed", long)]
    seed: Option<u64>,
    /// The bonded balance each bonded address receives. Not used if `--bonded-balances` is passed.
    #[clap(name = "bonded-balance", long, default_value_t = 10_000_000_000_000)]
    bonded_balance: u64,
    /// An optional map from address to bonded balance. Overrides `--bonded-balance` and `--committee-size`.
    #[clap(name = "bonded-balances", long)]
    bonded_balances: Option<Balances>,
    /// A place to optionally write out the generated committee private keys JSON.
    #[clap(name = "committee-file", long)]
    committee_file: Option<PathBuf>,
    /// Optionally initialize a ledger as well.
    #[clap(name = "ledger", long)]
    ledger: Option<PathBuf>,
}

impl Genesis {
    pub fn parse(self) -> Result<()> {
        let mut rng = ChaChaRng::seed_from_u64(self.seed.unwrap_or(DEVELOPMENT_MODE_RNG_SEED));

        // Generate a genesis key if one was not passed.
        let genesis_key = match self.genesis_key {
            Some(genesis_key) => genesis_key,
            None => PrivateKey::<MainnetV0>::new(&mut rng)?,
        };

        let genesis_addr = Address::try_from(&genesis_key)?;

        let (mut committee_members, bonded_balances, members, mut public_balances) = match self
            .bonded_balances
        {
            Some(balances) => {
                ensure!(
                    balances.0.contains_key(&genesis_addr),
                    "The genesis address should be present in the passed-in bonded balances."
                );

                let mut bonded_balances = IndexMap::with_capacity(self.committee_size as usize);
                let mut members = IndexMap::with_capacity(self.committee_size as usize);

                for (addr, balance) in &balances.0 {
                    ensure!(
                        balance >= &MIN_VALIDATOR_STAKE,
                        "Validator stake is too low: {balance} < {MIN_VALIDATOR_STAKE}",
                    );

                    bonded_balances.insert(*addr, (*addr, *balance));
                    members.insert(*addr, (*balance, true));
                }

                (None, bonded_balances, members, balances.0)
            }

            None => {
                ensure!(
                    self.bonded_balance >= MIN_VALIDATOR_STAKE,
                    "Validator stake is too low: {} < {MIN_VALIDATOR_STAKE}",
                    self.bonded_balance
                );

                let mut committee_members = IndexMap::with_capacity(self.committee_size as usize);
                let mut bonded_balances = IndexMap::with_capacity(self.committee_size as usize);
                let mut members = IndexMap::with_capacity(self.committee_size as usize);
                let mut public_balances = IndexMap::with_capacity(self.committee_size as usize);

                for i in 0..self.committee_size {
                    let (key, addr) = match i {
                        0 => (genesis_key, genesis_addr),
                        _ => {
                            let key = PrivateKey::<MainnetV0>::new(&mut rng)?;
                            let addr = Address::try_from(&key)?;

                            (key, addr)
                        }
                    };

                    committee_members.insert(addr, (key, self.bonded_balance));
                    bonded_balances.insert(addr, (addr, self.bonded_balance));
                    members.insert(addr, (self.bonded_balance, true));
                    public_balances.insert(addr, self.bonded_balance);
                }

                (
                    Some(committee_members),
                    bonded_balances,
                    members,
                    public_balances,
                )
            }
        };

        // Construct the committee.
        let committee = Committee::<MainnetV0>::new(0u64, members)?;

        // Add additional accounts to the public balances
        let accounts = (0..self.additional_accounts)
            .map(|_| {
                // Repeatedly regenerate key/addresses, ensuring they are not in `bonded_balances`.
                let (key, addr) = loop {
                    let key = PrivateKey::<MainnetV0>::new(&mut rng)?;
                    let addr = Address::try_from(&key)?;
                    if !bonded_balances.contains_key(&addr) {
                        break (key, addr);
                    }
                };

                public_balances.insert(addr, self.additional_accounts_balance);
                Ok((addr, (key, self.additional_accounts_balance)))
            })
            .collect::<Result<IndexMap<_, _>>>()?;

        // Calculate the public balance per validator.
        let remaining_balance = MainnetV0::STARTING_SUPPLY
            .saturating_sub(committee.total_stake())
            .saturating_sub(public_balances.values().sum());

        if remaining_balance > 0 {
            let balance = public_balances.get_mut(&genesis_addr).unwrap();
            *balance += remaining_balance;

            if let Some(ref mut committee_members) = committee_members {
                let (_, balance) = committee_members.get_mut(&genesis_addr).unwrap();
                *balance += remaining_balance;
            }
        }

        // Check if the sum of committee stakes and public balances equals the total starting supply.
        let public_balances_sum: u64 = public_balances.values().sum();
        if committee.total_stake() + public_balances_sum != MainnetV0::STARTING_SUPPLY {
            println!(
                "Sum of committee stakes and public balances does not equal total starting supply:
                                {} + {public_balances_sum} != {}",
                committee.total_stake(),
                MainnetV0::STARTING_SUPPLY
            );
        }

        // Construct the genesis block.
        let compute_span = tracing::span!(tracing::Level::ERROR, "compute span").entered();
        let block = load_or_compute_genesis(
            genesis_key,
            committee,
            public_balances,
            bonded_balances,
            &mut rng,
        )?;
        compute_span.exit();

        println!();

        // Write the genesis block.
        block.write_le(
            fs::File::options()
                .append(false)
                .create(true)
                .write(true)
                .open(&self.output)?,
        )?;

        // Print the genesis block private key if we generated one.
        if self.genesis_key.is_none() {
            println!(
                "The genesis block private key is: {}",
                genesis_key.to_string().cyan()
            );
        }

        // Print some info about the new genesis block.
        println!(
            "Genesis block written to {}.",
            self.output.display().to_string().yellow()
        );

        match (self.additional_accounts, self.additional_accounts_file) {
            // Don't display anything if we didn't make any additional accounts.
            (0, _) => (),

            // Write the accounts JSON file.
            (_, Some(accounts_file)) => {
                let file = fs::File::options()
                    .append(false)
                    .create(true)
                    .write(true)
                    .open(&accounts_file)?;
                serde_json::to_writer_pretty(file, &accounts)?;

                println!(
                    "Additional accounts written to {}.",
                    accounts_file.display().to_string().yellow()
                );
            }

            // Write the accounts to stdout if no file was passed.
            (_, None) => {
                println!(
                    "Additional accounts (each given {} balance):",
                    self.additional_accounts_balance
                );
                for (addr, (key, _)) in accounts {
                    println!(
                        "\t{}: {}",
                        addr.to_string().yellow(),
                        key.to_string().cyan()
                    );
                }
            }
        }

        // Display committee information if we generated it.
        match (committee_members, self.committee_file) {
            // file was passed
            (Some(committee_members), Some(committee_file)) => {
                let file = fs::File::options()
                    .append(false)
                    .create(true)
                    .write(true)
                    .open(&committee_file)?;
                serde_json::to_writer_pretty(file, &committee_members)?;

                println!(
                    "Generated committee written to {}.",
                    committee_file.display().to_string().yellow()
                );
            }

            // file was not passed
            (Some(committee_members), None) => {
                println!("Generated committee:");
                for (addr, (key, _)) in committee_members {
                    println!(
                        "\t{}: {}",
                        addr.to_string().yellow(),
                        key.to_string().cyan()
                    );
                }
            }

            _ => (),
        }

        // Initialize the ledger if a path was given.
        if let Some(ledger) = self.ledger {
            Ledger::<_, ConsensusDB<_>>::load(
                block.to_owned(),
                StorageMode::Custom(ledger.to_owned()),
            )?;
            println!(
                "Initialized a ledger at {}.",
                ledger.display().to_string().yellow()
            );
        }

        println!();
        println!("Genesis block hash: {}", block.hash().to_string().yellow());

        Ok(())
    }
}
