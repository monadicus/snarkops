use std::{fmt::Display, path::PathBuf, str::FromStr};

use clap::Parser;
use url::Url;

#[derive(Debug, Parser)]
pub struct Cli {
    /// Control plane server port
    #[arg(long, default_value_t = 1234)]
    pub port: u16,

    // TODO: store services in a file config or something?
    /// Optional URL referencing a Prometheus server
    #[arg(long)]
    pub prometheus: Option<Url>,

    // TODO: clarify that this needs to be an IP that agents can reach (handle external/internal?)
    /// Optional URL referencing a Loki server
    #[arg(long)]
    pub loki: Option<Url>,

    #[arg(long, default_value_t = PrometheusLocation::Docker)]
    pub prometheus_location: PrometheusLocation,

    /// Path to the directory containing the stored data
    #[arg(long, default_value = "snops-control-data")]
    pub path: PathBuf,

    #[arg(long)]
    pub hostname: Option<String>,
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
