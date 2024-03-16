use std::{fs, path::PathBuf, str::FromStr};

use aleo_std::StorageMode;
use anyhow::{ensure, Result};
use clap::Parser;
use colored::Colorize;
use indexmap::IndexMap;
use rand::SeedableRng;
use rand_chacha::ChaChaRng;
use serde::{Deserialize, Serialize};
use serde_clap_deserialize::serde_clap_default;
use snarkos_cli::commands::load_or_compute_genesis;
use snarkvm::{
    ledger::{
        committee::{Committee, MIN_VALIDATOR_STAKE},
        store::{helpers::memory::ConsensusMemory, ConsensusStore},
    },
    prelude::Network as _,
    utilities::ToBytes,
};

use crate::{Address, DbLedger, Network, PrivateKey};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Balances(IndexMap<Address, u64>);
impl FromStr for Balances {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[serde_clap_default]
#[derive(Debug, Clone, Parser, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Genesis {
    /// The private key to use when generating the genesis block. Generates one
    /// randomly if not passed.
    #[clap(name = "genesis-key", short, long)]
    #[serde(rename = "key")]
    pub genesis_key: Option<PrivateKey>,

    /// Where to write the genesis block to.
    #[clap(name = "output", short, long)]
    #[serde_clap_default(PathBuf::from("genesis.block"))]
    pub output: PathBuf,

    /// The committee size. Not used if --bonded-balances is set.
    #[clap(name = "committee-size", long)]
    #[serde_clap_default(4)]
    pub committee_size: u16,

    /// A place to optionally write out the generated committee private keys
    /// JSON.
    #[clap(name = "committee-output", long)]
    pub committee_output: Option<PathBuf>,

    /// Additional number of accounts that aren't validators to add balances to.
    #[clap(name = "additional-accounts", long)]
    #[serde_clap_default(0)]
    pub additional_accounts: u16,

    /// The balance to add to the number of accounts specified by
    /// additional-accounts.
    #[clap(name = "additional-accounts-balance", long)]
    #[serde_clap_default(100000000)] // 100_000_000
    pub additional_accounts_balance: u64,

    /// A place to write out the additionally generated accounts by
    /// --additional-accounts.
    #[clap(name = "additional-accounts-output", long)]
    pub additional_accounts_output: Option<PathBuf>,

    /// The seed to use when generating committee private keys and the genesis
    /// block. If unpassed, uses DEVELOPMENT_MODE_RNG_SEED (1234567890u64).
    #[clap(name = "seed", long)]
    pub seed: Option<u64>,

    /// The bonded balance each bonded address receives. Not used if
    /// `--bonded-balances` is passed.
    #[clap(name = "bonded-balance", long)]
    #[serde_clap_default(10000000000000)] // 10_000_000_000_000
    pub bonded_balance: u64,

    /// An optional map from address to bonded balance. Overrides
    /// `--bonded-balance` and `--committee-size`.
    #[clap(name = "bonded-balances", long)]
    pub bonded_balances: Option<Balances>,

    /// Optionally initialize a ledger as well.
    #[clap(name = "ledger", long)]
    #[serde(skip)]
    pub ledger: Option<PathBuf>,
}

impl Genesis {
    pub fn parse(self) -> Result<()> {
        let mut rng = ChaChaRng::seed_from_u64(self.seed.unwrap_or(1234567890u64));

        // Generate a genesis key if one was not passed.
        let genesis_key = match self.genesis_key {
            Some(genesis_key) => genesis_key,
            None => PrivateKey::new(&mut rng)?,
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

                    bonded_balances.insert(*addr, (*addr, *addr, *balance));
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
                            let key = PrivateKey::new(&mut rng)?;
                            let addr = Address::try_from(&key)?;

                            (key, addr)
                        }
                    };

                    committee_members.insert(addr, (key, self.bonded_balance));
                    bonded_balances.insert(addr, (addr, addr, self.bonded_balance));
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
        let committee = Committee::<Network>::new(0u64, members)?;

        // Add additional accounts to the public balances
        let accounts = (0..self.additional_accounts)
            .map(|_| {
                // Repeatedly regenerate key/addresses, ensuring they are not in
                // `bonded_balances`.
                let (key, addr) = loop {
                    let key = PrivateKey::new(&mut rng)?;
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
        let remaining_balance = Network::STARTING_SUPPLY
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

        // Check if the sum of committee stakes and public balances equals the total
        // starting supply.
        let public_balances_sum: u64 = public_balances.values().sum();
        if committee.total_stake() + public_balances_sum != Network::STARTING_SUPPLY {
            println!(
                "Sum of committee stakes and public balances does not equal total starting supply:
                                {} + {public_balances_sum} != {}",
                committee.total_stake(),
                Network::STARTING_SUPPLY
            );
        }

        // Construct the genesis block.
        let compute_span = tracing::span!(tracing::Level::ERROR, "compute span").entered();

        // Initialize a new VM.
        let vm = VM::from(ConsensusStore::<Network, ConsensusMemory<_>>::open(Some(
            0,
        ))?)?;
        // Initialize the genesis block.
        let block = vm.genesis_quorum(
            &genesis_key,
            committee,
            public_balances,
            bonded_balances,
            rng,
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

        match (self.additional_accounts, self.additional_accounts_output) {
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
        match (committee_members, self.committee_output) {
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
            DbLedger::load(block.to_owned(), StorageMode::Custom(ledger.to_owned()))?;
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
