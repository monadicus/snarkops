use std::{env, process::exit};

use anyhow::Result;
use clap::Parser;
use snarkos_aot::{cli::Cli, NetworkId};
use snarkvm::console::{
    network::{MainnetV0, TestnetV0},
    program::Network,
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() -> Result<()> {
    let network: NetworkId = env::var("NETWORK")
        .unwrap_or(NetworkId::Mainnet.to_string())
        .parse()
        .expect("Invalid network ID. Use 'mainnet' or 'testnet'.");

    match network {
        NetworkId::Mainnet => parse::<MainnetV0>(),
        NetworkId::Testnet => parse::<TestnetV0>(),
    }
}

fn parse<N: Network>() -> Result<()> {
    let cli = Cli::<N>::parse();

    if let Err(err) = cli.run() {
        eprintln!("⚠️ {err}");
        exit(1);
    }

    Ok(())
}
