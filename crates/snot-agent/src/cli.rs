use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use clap::Parser;
use snot_common::state::{AgentId, ModeConfig, PortConfig};

pub const ENV_ENDPOINT: &str = "SNOT_ENDPOINT";
pub const ENV_ENDPOINT_DEFAULT: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234);

// TODO: allow agents to define preferred internal/external addrs

#[derive(Debug, Parser)]
pub struct Cli {
    #[arg(long)]
    /// Control plane endpoint address
    pub endpoint: Option<SocketAddr>,

    #[arg(long)]
    pub id: Option<AgentId>,

    /// Path to the directory containing the stored data and configuration
    #[arg(long, default_value = "./snot-data")]
    pub path: PathBuf,

    /// Enable the agent to fetch its external address. Necessary to determine
    /// which agents are on shared networks, and for
    /// external-to-external connections
    #[arg(long)]
    pub external: bool,

    #[clap(long = "bind", default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
    pub bind_addr: IpAddr,

    #[clap(flatten)]
    pub ports: PortConfig,

    #[clap(flatten)]
    pub modes: ModeConfig,
}
