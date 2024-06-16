use anyhow::Result;
use clap::Args;
use snarkvm::console::program::Network;

use crate::DbLedger;

/// Used to initialize a new ledger given a genesis block.
#[derive(Debug, Args)]
pub struct Init;

impl Init {
    pub fn parse<N: Network>(self, ledger: &DbLedger<N>) -> Result<()> {
        let genesis_block = ledger.get_block(0)?;

        println!(
            "Ledger written, genesis block hash: {}",
            genesis_block.hash()
        );

        Ok(())
    }
}
