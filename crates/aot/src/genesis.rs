use std::{fs, path::PathBuf, str::FromStr};

use aleo_std::StorageMode;
use anyhow::{anyhow, ensure, Result};
use clap::Parser;
use colored::Colorize;
use indexmap::IndexMap;
use rand::{CryptoRng, Rng, SeedableRng};
use rand_chacha::ChaChaRng;
use serde::{de::DeserializeOwned, Serialize};
use snarkvm::{
    ledger::{
        committee::MIN_VALIDATOR_STAKE,
        store::{helpers::memory::ConsensusMemory, ConsensusStore},
        Header, Ratify, Solutions,
    },
    synthesizer::program::FinalizeGlobalState,
    utilities::ToBytes,
};

use crate::{
    ledger::util::public_transaction, Address, Block, CTRecord, Committee, DbLedger, MemVM,
    Network, NetworkId, PTRecord, PrivateKey, Transaction, ViewKey,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AddressMap<N: Network, T: DeserializeOwned>(IndexMap<Address<N>, T>);
impl<N: Network, T: DeserializeOwned> FromStr for AddressMap<N, T> {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(AddressMap(serde_json::from_str(s)?))
    }
}

/// This command helps generate a custom genesis block given an initial private
/// key, seed, and committee size.
#[derive(Debug, Clone, Parser)]
pub struct Genesis<N: Network> {
    /// The private key to use when generating the genesis block. Generates one
    /// randomly if not passed.
    #[clap(env, short, long)]
    pub genesis_key: Option<PrivateKey<N>>,

    /// Where to write the genesis block to.
    #[clap(short, long, default_value = "genesis.block")]
    pub output: PathBuf,

    /// The committee size. Not used if --bonded-balances is set.
    #[clap(long, default_value_t = 4)]
    pub committee_size: u16,

    /// A place to optionally write out the generated committee private keys
    /// JSON.
    #[clap(long)]
    pub committee_output: Option<PathBuf>,

    /// Additional number of accounts that aren't validators to add balances to.
    #[clap(long, default_value_t = 0)]
    pub additional_accounts: u16,

    /// The balance to add to the number of accounts specified by
    /// additional-accounts.
    #[clap(
        name = "additional-accounts-balance",
        long,
        default_value_t = 100_000_000
    )]
    pub additional_accounts_balance: u64,

    /// If --additional-accounts is passed you can additionally add an amount to
    /// give them in a record.
    #[clap(long)]
    pub additional_accounts_record_balance: Option<u64>,

    /// A place to write out the additionally generated accounts by
    /// --additional-accounts.
    #[clap(long)]
    pub additional_accounts_output: Option<PathBuf>,

    /// The seed to use when generating committee private keys and the genesis
    /// block. If unpassed, uses DEVELOPMENT_MODE_RNG_SEED (1234567890u64).
    #[clap(long)]
    pub seed: Option<u64>,

    /// The bonded balance each bonded address receives. Not used if
    /// `--bonded-balances` is passed.
    #[clap(long, default_value_t = 10_000_000_000_000)]
    pub bonded_balance: u64,

    /// An optional map from address to bonded balance. Overrides
    /// `--bonded-balance` and `--committee-size`.
    #[clap(long)]
    pub bonded_balances: Option<AddressMap<N, u64>>,

    /// An optional to specify withdrawal addresses for the genesis committee.
    #[clap(long)]
    pub bonded_withdrawal: Option<AddressMap<N, Address<N>>>,

    /// The bonded commission each bonded address uses. Not used if
    /// `--bonded-commissions` is passed. Defaults to 0. Must be 100 or less.
    #[clap(long, default_value_t = 0)]
    pub bonded_commission: u8,

    /// An optional map from address to bonded commission. Overrides
    /// `--bonded-commission`.
    /// Defaults to 0. Must be 100 or less.
    #[clap(long)]
    pub bonded_commissions: Option<AddressMap<N, u8>>,

    /// Optionally initialize a ledger as well.
    #[clap(long)]
    pub ledger: Option<PathBuf>,
}

/// Returns a new genesis block for a quorum chain.
pub fn genesis_quorum<R: Rng + CryptoRng, N: Network>(
    vm: &MemVM<N>,
    private_key: &PrivateKey<N>,
    committee: Committee<N>,
    public_balances: IndexMap<Address<N>, u64>,
    bonded_balances: IndexMap<Address<N>, (Address<N>, Address<N>, u64)>,
    transactions: Vec<Transaction<N>>,
    rng: &mut R,
) -> Result<Block<N>> {
    // Retrieve the total stake.
    let total_stake = committee.total_stake();
    // Compute the account supply.
    let account_supply = public_balances.values().try_fold(0u64, |acc, x| {
        acc.checked_add(*x).ok_or(anyhow!("Invalid account supply"))
    })?;
    // Compute the total supply.
    let total_supply = total_stake
        .checked_add(account_supply)
        .ok_or_else(|| anyhow!("Invalid total supply"))?;
    // Ensure the total supply matches.
    ensure!(
        total_supply == N::STARTING_SUPPLY,
        "Invalid total supply. Found {total_supply}, expected {}",
        N::STARTING_SUPPLY
    );

    // Prepare the ratifications.
    let ratifications = vec![Ratify::Genesis(
        Box::new(committee),
        Box::new(public_balances),
        Box::new(bonded_balances),
    )];
    // Prepare the solutions.
    let solutions = Solutions::from(None);
    // The genesis block
    // Prepare the aborted solution IDs.
    let aborted_solution_ids = vec![];

    // Construct the finalize state.
    let state = FinalizeGlobalState::new_genesis::<N>()?;
    // Speculate on the ratifications, solutions, and transactions.
    let (ratifications, transactions, aborted_transaction_ids, ratified_finalize_operations) = vm
        .speculate(
        state,
        None,
        ratifications,
        &solutions,
        transactions.iter(),
        rng,
    )?;
    ensure!(
        aborted_transaction_ids.is_empty(),
        "Failed to initialize a genesis block - found aborted transactionIDs"
    );

    // Prepare the block header.
    let header = Header::genesis(&ratifications, &transactions, ratified_finalize_operations)?; // Prepare the previous block hash.
    let previous_hash = N::BlockHash::default();

    // Construct the block.
    let block = Block::new_beacon(
        private_key,
        previous_hash,
        header,
        ratifications,
        solutions,
        aborted_solution_ids,
        transactions,
        aborted_transaction_ids,
        rng,
    )?;

    Ok(block)
}

impl<N: Network> Genesis<N> {
    pub fn parse(self) -> Result<()> {
        let mut rng = ChaChaRng::seed_from_u64(self.seed.unwrap_or(1234567890u64));

        tracing::trace!(
            "Generating genesis block for network {}",
            NetworkId::from_network::<N>()
        );

        // Generate a genesis key if one was not passed.
        let genesis_key = match self.genesis_key {
            Some(genesis_key) => genesis_key,
            None => PrivateKey::new(&mut rng)?,
        };

        let genesis_addr = Address::try_from(&genesis_key)?;

        // Lookup the commission for a given address.
        let get_commission = |addr| {
            self.bonded_commissions
                .as_ref()
                .and_then(|commissions| commissions.0.get(&addr))
                .copied()
                .unwrap_or(self.bonded_commission)
                .clamp(0, 100)
        };

        // Lookup the withdrawal address for a given address.
        let get_withdrawal = |addr| {
            self.bonded_withdrawal
                .as_ref()
                .and_then(|withdrawals| withdrawals.0.get(&addr))
                .copied()
                .unwrap_or(addr)
        };

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

                    bonded_balances.insert(*addr, (*addr, get_withdrawal(*addr), *balance));
                    members.insert(*addr, (*balance, true, get_commission(*addr)));
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
                    bonded_balances.insert(addr, (addr, get_withdrawal(addr), self.bonded_balance));
                    members.insert(addr, (self.bonded_balance, true, get_commission(addr)));
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
        let committee = Committee::new(0u64, members)?;

        // Add additional accounts to the public balances
        type Accounts<N> = IndexMap<Address<N>, (PrivateKey<N>, u64, Option<PTRecord<N>>)>;
        let mut accounts: Accounts<N> = (0..self.additional_accounts)
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

                let balance = self.additional_accounts_balance
                    + self.additional_accounts_record_balance.unwrap_or(0);

                public_balances.insert(addr, balance);
                Ok((addr, (key, balance, None)))
            })
            .collect::<Result<IndexMap<_, _>>>()?;

        // Calculate the public balance per validator.
        let remaining_balance = N::STARTING_SUPPLY
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
        if committee.total_stake() + public_balances_sum != N::STARTING_SUPPLY {
            println!(
                "Sum of committee stakes and public balances does not equal total starting supply:
                                {} + {public_balances_sum} != {}",
                committee.total_stake(),
                N::STARTING_SUPPLY
            );
        }

        // Construct the genesis block.
        let compute_span = tracing::span!(tracing::Level::ERROR, "compute span").entered();

        // Initialize a new VM.
        let vm = snarkvm::synthesizer::VM::from(ConsensusStore::<N, ConsensusMemory<_>>::open(
            Some(0),
        )?)?;

        // region: Genesis Records
        let mut txs = Vec::with_capacity(accounts.len());
        if let Some(record_balance) = self.additional_accounts_record_balance {
            accounts = accounts
                .into_iter()
                .map(|(addr, (key, balance, _))| {
                    let record_tx: Transaction<N> =
                        public_transaction::<N, ConsensusMemory<_>, N::Circuit>(
                            "transfer_public_to_private",
                            &vm,
                            addr,
                            record_balance,
                            key,
                            None,
                        )?;
                    // Cannot fail because transfer_public_to_private always emits a
                    // record.
                    let record_enc: CTRecord<N> = record_tx.records().next().unwrap().1.clone();
                    // Decrypt the record.
                    let record = record_enc.decrypt(&ViewKey::try_from(key)?)?;

                    txs.push(record_tx);
                    Ok((addr, (key, balance, Some(record))))
                })
                .collect::<Result<_>>()?;
        }

        // endregion: Genesis Records

        // Initialize the genesis block.
        let block = genesis_quorum(
            &vm,
            &genesis_key,
            committee,
            public_balances,
            bonded_balances,
            txs,
            &mut rng,
        )?;

        compute_span.exit();

        println!();

        // Write the genesis block.
        block.write_le(
            fs::File::options()
                .append(false)
                .truncate(true)
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

        let output_json = self.output.with_extension("json");
        serde_json::to_writer_pretty(
            fs::File::options()
                .append(false)
                .truncate(true)
                .create(true)
                .write(true)
                .open(&output_json)?,
            &block,
        )?;

        println!(
            "Genesis block JSON written to {}.",
            output_json.display().to_string().yellow()
        );

        match (self.additional_accounts, self.additional_accounts_output) {
            // Don't display anything if we didn't make any additional accounts.
            (0, _) => (),

            // Write the accounts JSON file.
            (_, Some(accounts_file)) => {
                let file = fs::File::options()
                    .append(false)
                    .create(true)
                    .truncate(true)
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
                for (addr, (key, _, _)) in accounts {
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
                    .truncate(true)
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
