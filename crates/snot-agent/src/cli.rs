use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use clap::Parser;
use http::Uri;
use snot_common::state::{AgentId, AgentMode, PortConfig};

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

    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub labels: Option<Vec<String>>,

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
    pub modes: AgentMode,
}

impl Cli {
    pub fn endpoint_and_uri(&self) -> (SocketAddr, Uri) {
        // get the endpoint
        let endpoint = self
            .endpoint
            .or_else(|| {
                env::var(ENV_ENDPOINT)
                    .ok()
                    .and_then(|s| s.as_str().parse().ok())
            })
            .unwrap_or(ENV_ENDPOINT_DEFAULT);

        let mut query = format!("/agent?mode={}", u8::from(self.modes));

        // add ?id=
        if let Some(id) = self.id {
            query.push_str(&format!("&id={}", id));
        }

        // add ?labels= or &labels= if id is present
        if let Some(labels) = &self.labels {
            query.push_str(&format!("&labels={}", labels.join(",")));
        }

        let ws_uri = Uri::builder()
            .scheme("ws")
            .authority(endpoint.to_string())
            .path_and_query(query)
            .build()
            .unwrap();

        (endpoint, ws_uri)
    }
}
