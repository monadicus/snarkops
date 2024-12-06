use anyhow::Result;
use clap::{Parser, ValueHint};

#[derive(Debug, Parser)]
#[clap(name = "snops-cli", author = "MONADIC.US")]
pub struct Cli {
    /// The url the control plane is on.
    #[clap(short, long, default_value = "http://localhost:1234", value_hint = ValueHint::Url)]
    url: String,
    /// The subcommand to run.
    #[clap(subcommand)]
    pub subcommand: crate::Commands,
}

impl Cli {
    /// Runs the subcommand.
    pub async fn run(self) -> Result<()> {
        self.subcommand.run(&self.url).await
    }
}
