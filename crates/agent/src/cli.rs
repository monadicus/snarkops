#[cfg(any(feature = "clipages", feature = "mangen"))]
use std::env;
use std::{
    fs,
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
};

#[cfg(any(feature = "clipages", feature = "mangen"))]
use clap::CommandFactory;
use clap::Parser;
use http::Uri;
use snops_common::state::{AgentId, AgentModeOptions, NetworkId, PortConfig, StorageId};
use tracing::{info, warn};

use crate::net;

// TODO: allow agents to define preferred internal/external addrs

#[derive(Debug, Parser)]
pub struct Cli {
    #[clap(long, env = "SNOPS_ENDPOINT", default_value = "127.0.0.1:1234")]
    /// Control plane endpoint address (IP, or wss://host, http://host)
    pub endpoint: String,

    /// Agent ID, used to identify the agent in the network.
    #[clap(long, env = "SNOPS_AGENT_ID")]
    pub id: AgentId,

    /// Locally provided private key file, used for envs where private keys are
    /// locally provided
    #[clap(long = "private-key-file", env = "SNOPS_AGENT_PRIVATE_KEY_FILE")]
    pub private_key_file: Option<PathBuf>,

    /// Labels to attach to the agent, used for filtering and grouping.
    #[clap(long, value_delimiter = ',', num_args = 1..)]
    pub labels: Option<Vec<String>>,

    /// Path to the directory containing the stored data and configuration
    #[clap(long, env = "SNOPS_AGENT_DATA_DIR", default_value = "./snops-data")]
    pub path: PathBuf,

    /// Enable the agent to fetch its external address. Necessary to determine
    /// which agents are on shared networks, and for
    /// external-to-external connections
    #[clap(long)]
    pub external: Option<IpAddr>,
    /// Manually specify internal addresses.
    #[clap(long)]
    pub internal: Option<IpAddr>,

    /// Bind address for the agent to listen on
    #[clap(long = "bind", env = "SNOPS_AGENT_HOST", default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
    pub bind_addr: IpAddr,

    #[clap(flatten)]
    pub ports: PortConfig,

    #[clap(flatten)]
    pub modes: AgentModeOptions,

    #[clap(short, long, default_value_t = false, env = "SNOPS_AGENT_QUIET")]
    /// Run the agent in quiet mode, suppressing most node output
    pub quiet: bool,

    #[cfg(any(feature = "clipages", feature = "mangen"))]
    #[clap(subcommand)]
    pub command: Commands,
}

#[cfg(any(feature = "clipages", feature = "mangen"))]
#[derive(Debug, Parser)]
pub enum Commands {
    #[cfg(feature = "mangen")]
    Man(snops_common::mangen::Mangen),
    #[cfg(feature = "clipages")]
    Md(snops_common::clipages::Clipages),
}

impl Cli {
    #[cfg(any(feature = "clipages", feature = "mangen"))]
    pub fn run(self) {
        match self.command {
            #[cfg(feature = "mangen")]
            Commands::Man(mangen) => {
                mangen
                    .run(
                        Cli::command(),
                        env!("CARGO_PKG_VERSION"),
                        env!("CARGO_PKG_NAME"),
                    )
                    .unwrap();
            }
            #[cfg(feature = "clipages")]
            Commands::Md(clipages) => {
                clipages.run::<Cli>(env!("CARGO_PKG_NAME")).unwrap();
            }
        }

        std::process::exit(0);
    }

    pub fn get_local_ip(&self) -> IpAddr {
        if self.bind_addr.is_unspecified() {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        } else {
            self.bind_addr
        }
    }

    pub fn endpoint_and_uri(&self) -> (String, Uri) {
        // get the endpoint
        let endpoint = &self.endpoint;

        let mut qs = url::form_urlencoded::Serializer::new(String::new());

        qs.append_pair("mode", &u8::from(self.modes).to_string());

        // Add agent version
        qs.append_pair("version", env!("CARGO_PKG_VERSION"));

        // add &id=
        qs.append_pair("id", self.id.as_ref());

        // add local pk flag
        if let Some(file) = self.private_key_file.as_ref() {
            if fs::metadata(file).is_ok() {
                qs.append_pair("local_pk", "true");
            } else {
                warn!("Private-key-file flag ignored as the file was not found: {file:?}")
            }
        }

        // add &labels= if id is present
        if let Some(labels) = &self.labels {
            info!("Using labels: {:?}", labels);
            qs.append_pair(
                "labels",
                &labels
                    .iter()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.trim())
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }

        let (is_tls, host) = endpoint
            .split_once("://")
            .map(|(left, right)| (left == "wss" || left == "https", right))
            .unwrap_or((false, endpoint.as_str()));

        let addr = format!("{host}{}", if host.contains(':') { "" } else { ":1234" });

        let ws_uri = Uri::builder()
            .scheme(if is_tls { "wss" } else { "ws" })
            .authority(addr.to_owned())
            .path_and_query(format!("/agent?{}", qs.finish()))
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

    pub fn addrs(&self) -> (Vec<IpAddr>, Option<IpAddr>) {
        let internal_addrs = match (self.internal, self.external) {
            // use specified internal address
            (Some(internal), _) => vec![internal],
            // use no internal address if the external address is loopback
            (None, Some(external)) if external.is_loopback() => vec![],
            // otherwise, get the local network interfaces available to this node
            (None, _) => net::get_internal_addrs().expect("failed to get network interfaces"),
        };

        let external_addr = self.external;
        if let Some(addr) = external_addr {
            info!("Using external addr: {}", addr);
        } else {
            info!("Skipping external addr");
        }

        (internal_addrs, external_addr)
    }

    pub fn storage_path(&self, network: NetworkId, storage_id: StorageId) -> PathBuf {
        let mut path = self.path.join("storage");
        path.push(network.to_string());
        path.push(storage_id.to_string());
        path
    }
}
