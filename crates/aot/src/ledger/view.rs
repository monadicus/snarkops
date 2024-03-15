use std::str::FromStr;

use anyhow::Result;
use clap::Subcommand;

use crate::{ledger::util, Address, DbLedger};

#[derive(Debug, Subcommand)]
pub enum View {
    Block { block_height: u32 },
    Balance { address: String },
}

impl View {
    pub fn parse(self, ledger: &DbLedger) -> Result<()> {
        match self {
            View::Block { block_height } => {
                // Print information about the ledger
                println!("{:#?}", ledger.get_block(block_height)?);
            }
            View::Balance { address } => {
                let addr = Address::from_str(&address)?;

                println!("{address} balance {}", util::get_balance(addr, &ledger)?);
            }
        }
        Ok(())
    }
}
