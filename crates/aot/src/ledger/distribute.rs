use anyhow::{bail, Result};
use clap::Args;
use indicatif::ProgressIterator;
use rand::SeedableRng;
use rand_chacha::ChaChaRng;
use snarkvm::circuit::AleoV0;

use super::Accounts;
use crate::{ledger::util, Address, DbLedger, PrivateKey, VM};

#[derive(Debug, Args)]
pub struct Distribute {
    /// The private key in which to distribute credits from.
    #[arg(required = true, long)]
    from: PrivateKey,
    /// A comma-separated list of addresses to distribute credits to. This or
    /// `--num-accounts` must be passed.
    #[arg(long, conflicts_with = "num_accounts")]
    to: Option<Accounts>,
    /// The number of new addresses to generate and distribute credits to. This
    /// or `--to` must be passed.
    #[arg(long, conflicts_with = "to")]
    num_accounts: Option<u32>,
    /// The amount of microcredits to distribute.
    #[arg(long)]
    amount: u64,
}

impl Distribute {
    pub fn parse(self, ledger: &DbLedger) -> Result<()> {
        let mut rng = ChaChaRng::from_entropy();

        // Determine the accounts to distribute to
        let to = match (self.to, self.num_accounts) {
            // Addresses explicitly passed
            (Some(to), None) => to.0,

            // No addresses passed, generate private keys at runtime
            (None, Some(num)) => (0..num)
                .map(|_| Address::try_from(PrivateKey::new(&mut rng)?))
                .collect::<Result<Vec<_>>>()?,

            // Cannot pass both/neither
            _ => bail!("must specify only ONE of --to and --num-accounts"),
        };

        let max_transactions = VM::MAXIMUM_CONFIRMED_TRANSACTIONS;
        let per_account = self.amount / to.len() as u64;

        // Generate a transaction for each address
        let transactions = to
            .iter()
            .progress_count(to.len() as u64)
            .map(|addr| {
                util::make_transaction_proof::<_, _, AleoV0>(
                    ledger.vm(),
                    *addr,
                    per_account,
                    self.from,
                    None,
                )
            })
            .filter_map(Result::ok)
            .collect::<Vec<_>>();

        // Add the transactions into blocks
        let num_blocks = util::add_transaction_blocks(
            ledger,
            self.from,
            &transactions,
            max_transactions,
            &mut rng,
        )?;

        println!(
            "Created {num_blocks} from {} transactions ({} failed).",
            transactions.len(),
            to.len() - transactions.len()
        );

        Ok(())
    }
}
