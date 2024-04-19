use std::{
    env, fs,
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
};

use clap::Parser;
use http::Uri;
use snops_common::state::{AgentId, AgentMode, PortConfig};
use tracing::{info, warn};

pub const ENV_ENDPOINT: &str = "SNOPS_ENDPOINT";
pub const ENV_ENDPOINT_DEFAULT: &str = "127.0.0.1:1234";

// TODO: allow agents to define preferred internal/external addrs

#[derive(Debug, Parser)]
pub struct Cli {
    #[arg(long)]
    /// Control plane endpoint address (IP, or wss://host, http://host)
    pub endpoint: Option<String>,

    #[arg(long)]
    pub id: Option<AgentId>,

    /// Locally provided private key file, used for envs where private keys are
    /// locally provided
    #[arg(long)]
    #[clap(long = "private-key-file")]
    pub private_key_file: Option<PathBuf>,

    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub labels: Option<Vec<String>>,

    /// Path to the directory containing the stored data and configuration
    #[arg(long, default_value = "./snops-data")]
    pub path: PathBuf,

    /// Enable the agent to fetch its external address. Necessary to determine
    /// which agents are on shared networks, and for
    /// external-to-external connections
    #[arg(long)]
    pub external: Option<IpAddr>,

    #[clap(long = "bind", default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
    pub bind_addr: IpAddr,

    #[clap(flatten)]
    pub ports: PortConfig,

    #[clap(flatten)]
    pub modes: AgentMode,
}

impl Cli {
    pub fn endpoint_and_uri(&self) -> (String, Uri) {
        // get the endpoint
        let endpoint = self
            .endpoint
            .as_ref()
            .cloned()
            .or_else(|| env::var(ENV_ENDPOINT).ok())
            .unwrap_or(ENV_ENDPOINT_DEFAULT.to_owned());

        let mut query = format!("/agent?mode={}", u8::from(self.modes));

        // add &id=
        if let Some(id) = self.id {
            query.push_str(&format!("&id={}", id));
        }

        // add local pk flag
        if let Some(file) = self.private_key_file.as_ref() {
            if fs::metadata(file).is_ok() {
                query.push_str("&local_pk=true");
            } else {
                warn!("private-key-file flag ignored as the file was not found: {file:?}")
            }
        }

        // add &labels= if id is present
        if let Some(labels) = &self.labels {
            info!("using labels: {:?}", labels);
            query.push_str(&format!("&labels={}", labels.join(",")));
        }

        let (is_tls, host) = endpoint
            .split_once("://")
            .map(|(left, right)| (left == "wss" || left == "https", right))
            .unwrap_or((false, endpoint.as_str()));

        let addr = format!("{host}{}", if host.contains(':') { "" } else { ":1234" });

        let ws_uri = Uri::builder()
            .scheme(if is_tls { "wss" } else { "ws" })
            .authority(addr.to_owned())
            .path_and_query(query)
            .build()
            .unwrap();

        (
            format!(
                "{proto}://{addr}",
                proto = if is_tls { "https" } else { "http" },
            ),
            ws_uri,
        )
    }
}
