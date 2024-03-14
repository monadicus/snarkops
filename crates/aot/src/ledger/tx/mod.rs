use super::*;

mod ops;

#[derive(Debug, Subcommand)]
pub enum Tx {
    FromOps(ops::FromOps),
}

impl Tx {
    pub fn parse(self, ledger: &MemoryLedger) -> Result<()> {
        match self {
            Tx::FromOps(random) => random.parse(ledger),
        }
    }
}
