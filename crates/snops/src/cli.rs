use std::{
    fmt::Display,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};

#[cfg(any(feature = "clipages", feature = "mangen"))]
use clap::CommandFactory;
use clap::Parser;
use url::Url;

#[derive(Debug, Parser)]
pub struct Cli {
    #[clap(long = "bind", default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
    pub bind_addr: IpAddr,

    /// Control plane server port
    #[arg(long, default_value_t = 1234)]
    pub port: u16,

    // TODO: store services in a file config or something?
    /// Optional URL referencing a Prometheus server
    #[arg(long, env = "PROMETHEUS_URL")]
    pub prometheus: Option<Url>,

    // TODO: clarify that this needs to be an IP that agents can reach (handle external/internal?)
    /// Optional URL referencing a Loki server
    #[arg(long, env = "LOKI_URL")]
    pub loki: Option<Url>,

    #[arg(long, default_value_t = PrometheusLocation::Docker)]
    pub prometheus_location: PrometheusLocation,

    /// Path to the directory containing the stored data
    #[arg(long, default_value = "snops-control-data")]
    pub path: PathBuf,

    #[arg(long)]
    /// Hostname to advertise to the control plane, used when resolving the
    /// control plane's address for external cannons can be an external IP
    /// or FQDN, will have the port appended
    ///
    /// must contain http:// or https://
    pub hostname: Option<String>,

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
