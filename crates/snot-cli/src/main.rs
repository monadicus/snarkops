use std::process::exit;

use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    if let Err(err) = cli.run() {
        eprintln!("⚠️ {err}");
        exit(1);
    }

    Ok(())
}
