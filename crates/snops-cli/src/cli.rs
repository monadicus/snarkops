use anyhow::Result;
use clap::Parser;

#[derive(Debug, Parser)]
#[clap(name = "snops-cli", author = "MONADIC.US")]
pub struct Cli {
    /// The subcommand to run.
    #[clap(subcommand)]
    pub subcommand: crate::commands::Commands,
}

impl Cli {
    /// Runs the subcommand.
    pub fn run(self) -> Result<()> {
        self.subcommand.run()
    }
}
