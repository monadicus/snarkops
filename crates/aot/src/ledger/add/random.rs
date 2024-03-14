use super::*;

#[derive(Debug, Args)]
pub struct Random {
    #[arg(long)]
    block_private_key: Option<PrivateKey<Network>>,
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
    /// Maximum transaction credit transfer. If unspecified, maximum is entire account balance.
    #[arg(long)]
    max_tx_credits: Option<u64>,
}

impl Random {
    pub fn parse<R: Rng + CryptoRng>(self, ledger: &DbLedger, rng: &mut R) -> Result<()> {
        // TODO: do this for each block?
        let block_private_key = match self.block_private_key {
            Some(key) => key,
            None => PrivateKey::<Network>::new(rng)?,
        };

        let max_transactions = VM::<Network, Db>::MAXIMUM_CONFIRMED_TRANSACTIONS;

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

            let tx_span = span!(Level::INFO, "tx generation");
            let txs = (0..num_tx_per_block)
                .into_par_iter()
                .progress_count(num_tx_per_block as u64)
                .map(|_| {
                    let _enter = tx_span.enter();

                    let mut rng = ChaChaRng::from_rng(thread_rng())?;

                    let keys = self.private_keys.random_accounts(&mut rng);

                    let from = Address::try_from(keys[1])?;
                    let amount = match self.max_tx_credits {
                        Some(amount) => rng.gen_range(1..amount),
                        None => rng.gen_range(1..util::get_balance(from, &ledger)?),
                    };

                    let to = Address::try_from(keys[0])?;

                    let proof_span = span!(Level::INFO, "tx generation proof");
                    let _enter = proof_span.enter();

                    util::make_transaction_proof::<_, _, AleoV0>(
                        ledger.vm(),
                        to,
                        amount,
                        keys[1],
                        keys.get(2).copied(),
                    )
                })
                .filter_map(Result::ok)
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
