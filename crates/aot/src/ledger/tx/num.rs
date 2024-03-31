use anyhow::Result;
use clap::Args;
use indicatif::{ProgressBar, ProgressIterator};

use crate::{
    ledger::{util, PrivateKeys},
    MemoryLedger,
};

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
            let progress_bar = ProgressBar::new(remainder);
            progress_bar.tick();
            eprintln!("Attempting to gen {remainder} transactions.");

            count += util::gen_n_tx(ledger, &self.private_keys, remainder, None)
                .filter_map(Result::ok) // discard failed transactions
                // print each transaction to stdout
                .inspect(|proof| match proof {
                    util::CannonTx::Standalone(tx) => {
                        println!("{}", serde_json::to_string(&tx).expect("serialize proof"));
                    }
                    util::CannonTx::Dependent(id, tx) => {
                        // println!(
                        //     "{id}\t{}",
                        //     serde_json::to_string(&tx).expect("serialize
                        // proof") );
                    }
                })
                .map(drop)
                .progress_with(progress_bar)
                // take the count of succeeeded proofs
                .count();
        }

        eprintln!("Wrote {count} transactions.");

        Ok(())
    }
}
