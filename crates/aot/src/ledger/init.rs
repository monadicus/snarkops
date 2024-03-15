use anyhow::Result;
use clap::Args;

use crate::DbLedger;

#[derive(Debug, Args)]
pub struct Init;

impl Init {
    pub fn parse(self, ledger: &DbLedger) -> Result<()> {
        let genesis_block = ledger.get_block(0)?;

        println!(
            "Ledger written, genesis block hash: {}",
            genesis_block.hash()
        );

        Ok(())
    }
}
