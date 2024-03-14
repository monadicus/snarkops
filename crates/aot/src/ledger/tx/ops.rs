use super::*;

#[derive(Debug, Deserialize, Clone)]
pub struct TxOperation {
    from: PrivateKey,
    to: Address,
    amount: u64,
}

#[derive(Debug, Clone, Deserialize)]
/// This wrapper allows for '--operations=[{}, {}]' instead of '--operations {}
/// --operations {}'
pub struct TxOperations(pub Vec<TxOperation>);

impl FromStr for TxOperations {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Debug, Args)]
pub struct FromOps {
    #[arg(required = true, long)]
    operations: TxOperations,
}

impl FromOps {
    pub fn parse(self, ledger: &MemoryLedger) -> Result<()> {
        let progress_bar = ProgressBar::new(self.operations.0.len() as u64);
        progress_bar.tick();

        let gen_txs = self
            .operations
            .0
            // rayon for free parallelism
            .into_par_iter()
            // generate proofs
            .map(|op| {
                util::make_transaction_proof::<_, _, AleoV0>(
                    ledger.vm(),
                    op.to,
                    op.amount,
                    op.from,
                    None,
                )
            })
            // discard failed transactions
            .filter_map(Result::ok)
            // print each transaction to stdout
            .inspect(|proof| {
                println!(
                    "{}",
                    serde_json::to_string(&proof).expect("serialize proof")
                )
            })
            // progress bar
            .progress_with(progress_bar)
            // take the count of succeeeded proofs
            .count();

        eprintln!("Wrote {} transactions.", gen_txs);
        Ok(())
    }
}
