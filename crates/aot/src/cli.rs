use crate::ledger::Command as Ledger;
use anyhow::Result;
use clap::Parser;

use crate::genesis::Genesis;

#[derive(Debug, Parser)]
#[clap(name = "snarkOS AoT", author = "MONADIC.US")]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Parser)]
pub enum Command {
    #[clap(name = "genesis")]
    Genesis(Genesis),
    #[clap(name = "ledger")]
    Ledger(Ledger),
}

impl Command {
    pub fn parse(self) -> Result<()> {
        match self {
            Self::Genesis(command) => command.parse(),
            Self::Ledger(command) => command.parse(),
        }
    }
}
