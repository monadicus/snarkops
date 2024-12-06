use std::process::exit;

use anyhow::Result;
use clap::Parser;

mod cli;
pub(crate) use cli::*;

mod events;

mod commands;
pub(crate) use commands::*;

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let cli = cli::Cli::parse();

    if let Err(err) = cli.run().await {
        eprintln!("⚠️ {err:?}");
        exit(1);
    }

    Ok(())
}
