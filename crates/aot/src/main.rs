use std::{env, process::exit};

use anyhow::Result;
use clap::Parser;
use snarkos_aot::{cli::Cli, Network, NetworkId};
use snarkvm::console::network::{CanaryV0, MainnetV0, TestnetV0};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() -> Result<()> {
    let network: NetworkId = env::var("NETWORK")
        .unwrap_or(NetworkId::Mainnet.to_string())
        .parse()
        .expect("Invalid network ID. Use 'mainnet', 'testnet', or 'canary'.");

    match network {
        NetworkId::Mainnet => parse::<MainnetV0>(),
        NetworkId::Testnet => parse::<TestnetV0>(),
        NetworkId::Canary => parse::<CanaryV0>(),
    }
}

fn parse<N: Network>() -> Result<()> {
    let cli = Cli::<N>::parse();

    if let Err(err) = cli.run() {
        eprintln!("⚠️ {err:?}");
        exit(1);
    }

    Ok(())
}
