use anyhow::Result;
use clap::Subcommand;

use crate::MemoryLedger;

mod num;
mod ops;

// TODO: convert this to an auth generator, parallelized

#[derive(Debug, Subcommand)]
pub enum Tx {
    FromOps(ops::FromOps),
    Num(num::Num),
}

impl Tx {
    pub fn parse(self, ledger: &MemoryLedger) -> Result<()> {
        match self {
            Tx::FromOps(random) => random.parse(ledger),
            Tx::Num(num) => num.parse(ledger),
        }
    }
}
