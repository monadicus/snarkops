use std::process::exit;

use anyhow::Result;
use clap::Parser;
use snarkos_aot::cli::Cli;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Err(err) = cli.run() {
        eprintln!("⚠️ {err}");
        exit(1);
    }

    Ok(())
}
