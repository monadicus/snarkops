use std::{
    fmt::Display,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};

#[cfg(any(feature = "clipages", feature = "mangen"))]
use clap::CommandFactory;
use clap::Parser;
use clap_serde_derive::ClapSerde;
use serde::{de::Error, Deserialize};
use url::Url;

#[derive(Parser)]
pub struct Cli {
    /// A path to a config file. A config file is a YAML file; all config
    /// arguments are valid YAML fields.
    #[arg(short, long = "config")]
    pub config_path: Option<PathBuf>,

    #[command(flatten)]
    pub config: <Config as ClapSerde>::Opt,

    #[cfg(any(feature = "clipages", feature = "mangen"))]
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(ClapSerde, Debug)]
pub struct Config {
    #[default(IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
    #[clap(long = "bind")]
    pub bind_addr: IpAddr,

    /// Control plane server port
    #[default(1234)]
    #[arg(long)]
    pub port: u16,

    // TODO: store services in a file config or something?
    /// Optional URL referencing a Prometheus server
    #[arg(long, env = "PROMETHEUS_URL")]
    pub prometheus: Option<Url>,

    // TODO: clarify that this needs to be an IP that agents can reach (handle external/internal?)
    /// Optional URL referencing a Loki server
    #[arg(long, env = "LOKI_URL")]
    pub loki: Option<Url>,

    #[default(PrometheusLocation::Docker)]
    #[arg(long)]
    pub prometheus_location: PrometheusLocation,

    /// Path to the directory containing the stored data
    #[default(PathBuf::from("snops-control-data"))]
    #[arg(long)]
    pub path: PathBuf,

    /// Hostname to advertise to the control plane, used when resolving the
    /// control plane's address for external cannons can be an external IP
    /// or FQDN, will have the port appended
    ///
    /// must contain http:// or https://
    #[arg(long)]
    pub hostname: Option<String>,
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
}

impl Config {
    pub fn get_local_addr(&self) -> SocketAddr {
        let ip = if self.bind_addr.is_unspecified() {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        } else {
            self.bind_addr
        };
        SocketAddr::new(ip, self.port)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub enum PrometheusLocation {
    Internal,
    External,
    #[default]
    Docker,
}

impl Display for PrometheusLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use PrometheusLocation::*;

        match self {
            Internal => f.write_str("internal"),
            External => f.write_str("external"),
            Docker => f.write_str("docker"),
        }
    }
}

impl FromStr for PrometheusLocation {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use PrometheusLocation::*;

        Ok(match s {
            "internal" => Internal,
            "external" => External,
            "docker" => Docker,
            _ => return Err("expected one of 'internal', 'external', 'docker'"),
        })
    }
}

impl<'de> Deserialize<'de> for PrometheusLocation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        PrometheusLocation::from_str(<&str>::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}
