use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use clap::Parser;

pub const ENV_ENDPOINT: &str = "SNOT_ENDPOINT";
pub const ENV_ENDPOINT_DEFAULT: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234);

// TODO: allow agents to define preferred internal/external addrs

#[derive(Debug, Parser)]
pub struct Cli {
    #[arg(long)]
    /// Control plane endpoint address
    pub endpoint: Option<SocketAddr>,

    #[arg(long, default_value = "./snot-data")]
    /// Path to the directory containing the stored data and configuration
    pub path: PathBuf,

    #[arg(long, default_value_t = false)]
    /// Enable the agent to fetch its external address. Necessary to determine
    /// which agents are on shared networks, and for
    /// external-to-external connections
    pub external: bool,

    #[clap(long = "bind", default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
    pub bind_addr: IpAddr,
    /// Specify the IP address and port for the node server
    #[clap(long = "node", default_value_t = 4130)]
    pub node: u16,
    /// Specify the IP address and port for the BFT
    #[clap(long = "bft", default_value = "5000")]
    pub bft: u16,
    /// Specify the IP address and port for the REST server
    #[clap(long = "rest", default_value = "3030")]
    pub rest: u16,
    // TODO: specify allowed modes (--validator --client --tx-gen)
}
