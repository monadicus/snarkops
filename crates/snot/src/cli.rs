use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct Cli {
    #[arg(long, default_value = "1234")]
    /// Control plane server port
    pub port: u16,

    #[arg(long, default_value = "./snot-control-data")]
    /// Path to the directory containing the stored data
    pub path: PathBuf,
}
