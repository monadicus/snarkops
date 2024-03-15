use super::*;

#[derive(Debug, Args)]
pub struct Num {
    #[arg(required = true, long)]
    private_keys: PrivateKeys,
    pub num: u64,
}

impl Num {
    pub fn parse(self, ledger: &MemoryLedger) -> Result<()> {
        let mut count = 0;

        while count < self.num as usize {
            let remainder = self.num - count as u64;
            eprintln!("Attempting to gen {remainder} transactions.");
            let progress_bar = ProgressBar::new(remainder);
            progress_bar.tick();

            count += util::gen_n_tx(ledger, &self.private_keys, remainder, None)
                .filter_map(Result::ok) // discard failed transactions
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
        }

        eprintln!("Wrote {count} transactions.");

        Ok(())
    }
}
