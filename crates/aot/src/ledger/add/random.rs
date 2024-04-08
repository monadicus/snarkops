use anyhow::{ensure, Result};
use clap::Args;
use rand::{CryptoRng, Rng};

use crate::{
    ledger::{
        util::{self, CannonTx},
        PrivateKeys,
    },
    DbLedger, PrivateKey, VM,
};

#[derive(Debug, Args)]
pub struct Random {
    #[arg(long)]
    block_private_key: Option<PrivateKey>,
    #[arg(required = true, long)]
    private_keys: PrivateKeys,
    #[arg(short, long, default_value_t = 5)]
    num_blocks: u8,
    /// Minimum number of transactions per block.
    #[arg(long, default_value_t = 128)]
    min_per_block: usize,
    /// Maximumnumber of transactions per block.
    #[arg(long, default_value_t = 1024)]
    max_per_block: usize,
    /// Maximum transaction credit transfer. If unspecified, maximum is entire
    /// account balance.
    #[arg(long)]
    max_tx_credits: Option<u64>,
}

impl Random {
    pub fn parse<R: Rng + CryptoRng>(self, ledger: &DbLedger, rng: &mut R) -> Result<()> {
        // TODO: do this for each block?
        let block_private_key = match self.block_private_key {
            Some(key) => key,
            None => PrivateKey::new(rng)?,
        };

        let max_transactions = VM::MAXIMUM_CONFIRMED_TRANSACTIONS;

        ensure!(
            self.min_per_block <= max_transactions,
            "minimum is above max block txs (max is {max_transactions})"
        );

        ensure!(
            self.max_per_block <= max_transactions,
            "maximum is above max block txs (max is {max_transactions})"
        );

        let mut total_txs = 0;
        let mut gen_txs = 0;

        for _ in 0..self.num_blocks {
            let num_tx_per_block = rng.gen_range(self.min_per_block..=self.max_per_block);
            total_txs += num_tx_per_block;

            let txs = util::gen_n_tx(
                ledger,
                &self.private_keys,
                num_tx_per_block as u64,
                self.max_tx_credits,
                false,
            )
            .filter_map(Result::ok)
            .map(|tx| match tx {
                CannonTx::Standalone(tx) => tx,
                // TODO: ensure these transactions can be appended in the same block as the record
                // TODO: otherwise, will need to push these into the next block
                CannonTx::Dependent(_, tx) => tx,
            })
            .collect::<Vec<_>>();

            gen_txs += txs.len();
            let target_block = ledger.prepare_advance_to_next_beacon_block(
                &block_private_key,
                vec![],
                vec![],
                txs,
                rng,
            )?;

            ledger.advance_to_next_block(&target_block)?;
        }

        println!(
            "Generated {gen_txs} transactions ({} failed)",
            total_txs - gen_txs
        );

        Ok(())
    }
}
