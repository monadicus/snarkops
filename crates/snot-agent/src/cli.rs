use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use clap::Parser;

pub const ENV_ENDPOINT: &'static str = "SNOT_ENDPOINT";
pub const ENV_ENDPOINT_DEFAULT: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234);

#[derive(Debug, Parser)]
pub struct Cli {
    #[arg(long)]
    /// Control plane endpoint address
    pub endpoint: Option<SocketAddr>,

    #[arg(long, default_value = "./snot-data")]
    /// Path to the directory containing the stored data and configuration
    pub path: PathBuf,
    // TODO: specify allowed modes
}
