use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;

#[derive(Debug, Parser)]
pub struct Cli {
    /// Control plane server port
    #[arg(long, default_value_t = 1234)]
    pub port: u16,

    /// Optional IP:port that a Prometheus server is running on
    #[arg(long)]
    pub prometheus: Option<SocketAddr>,

    /// Path to the directory containing the stored data
    #[arg(long, default_value = "snot-control-data")]
    pub path: PathBuf,

    #[arg(long)]
    pub hostname: Option<String>,
}
