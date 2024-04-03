use core::str::FromStr;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use aleo_std::StorageMode;
use anyhow::{bail, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_clap_deserialize::serde_clap_default;
use snarkos_node::Node;
use snarkvm::{prelude::Block, utilities::FromBytes};
use snops_common::state::NodeType;

use crate::{ledger::Addrs, Account, PrivateKey};

mod metrics;

#[derive(Debug, Args, Serialize, Deserialize)]
#[group(required = true, multiple = false)]
pub struct Key {
    /// Specify the account private key of the node
    #[clap(long = "private-key")]
    pub private_key: Option<PrivateKey>,
    /// Specify the account private key of the node
    #[clap(long = "private-key-file")]
    pub private_key_file: Option<PathBuf>,
}

impl Key {
    pub fn try_get(self) -> Result<PrivateKey> {
        match (self.private_key, self.private_key_file) {
            (Some(key), None) => Ok(key),
            (None, Some(file)) => {
                let raw = std::fs::read_to_string(file)?.trim().to_string();
                Ok(PrivateKey::from_str(&raw)?)
            }
            // clap should make this unreachable, but serde might not
            _ => bail!("Either `private-key` or `private-key-file` must be set"),
        }
    }
}

#[serde_clap_default]
#[derive(Debug, Args, Serialize, Deserialize)]
pub struct Runner {
    /// A path to the genesis block to initialize the ledger from.
    #[arg(required = true, short, long, default_value = "genesis.block")]
    pub genesis: PathBuf,

    /// The ledger from which to view a block.
    #[arg(required = true, short, long, default_value = "./ledger")]
    pub ledger: PathBuf,

    #[arg(required = true, name = "type", short, long)]
    pub node_type: NodeType,

    #[clap(flatten)]
    pub key: Key,

    #[clap(long = "bind", default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
    pub bind_addr: IpAddr,
    /// Specify the IP address and port for the node server
    #[clap(long = "node", default_value_t = 4130)]
    pub node: u16,
    /// Specify the IP address and port for the BFT
    #[clap(long = "bft", default_value_t = 5000)]
    pub bft: u16,
    /// Specify the IP address and port for the REST server
    #[clap(long = "rest", default_value_t = 3030)]
    pub rest: u16,
    /// Specify the port for the metrics server
    #[clap(long = "metrics", default_value_t = 9000)]
    pub metrics: u16,

    /// Specify the IP address and port of the peer(s) to connect to
    #[clap(long = "peers", default_value = "")]
    pub peers: Addrs,
    /// Specify the IP address and port of the validator(s) to connect to
    #[clap(long = "validators", default_value = "")]
    pub validators: Addrs,
    /// Specify the requests per second (RPS) rate limit per IP for the REST
    /// server
    #[clap(long = "rest-rps", default_value_t = 1000)]
    pub rest_rps: u32,
}

impl Runner {
    #[tokio::main]
    pub async fn parse(self) -> Result<()> {
        let bind_addr = self.bind_addr;
        let node_ip = SocketAddr::new(bind_addr, self.node);
        let rest_ip = SocketAddr::new(bind_addr, self.rest);
        let bft_ip = SocketAddr::new(bind_addr, self.bft);
        let metrics_ip = SocketAddr::new(bind_addr, self.metrics);

        let account = Account::try_from(self.key.try_get()?)?;

        let genesis = Block::from_bytes_le(&std::fs::read(&self.genesis)?)?;
        let storage_mode = StorageMode::Custom(self.ledger);

        // slight alterations to the normal `metrics::initialize_metrics` because of
        // visibility issues
        {
            // Build the Prometheus exporter.
            metrics_exporter_prometheus::PrometheusBuilder::new()
                .with_http_listener(metrics_ip)
                .install()
                .expect("can't build the prometheus exporter");

            // Register the snarkVM metrics.
            snarkvm::metrics::register_metrics();

            // Register the metrics so they exist on init.
            for name in metrics::GAUGE_NAMES {
                ::metrics::register_gauge(name);
            }
            for name in metrics::COUNTER_NAMES {
                ::metrics::register_counter(name);
            }
            for name in metrics::HISTOGRAM_NAMES {
                ::metrics::register_histogram(name);
            }
        }

        match self.node_type {
            NodeType::Validator => {
                Node::new_validator(
                    node_ip,
                    Some(bft_ip),
                    Some(rest_ip),
                    self.rest_rps,
                    account,
                    &self.peers,
                    &self.validators,
                    genesis,
                    None,
                    storage_mode,
                    false,
                    false,
                )
                .await?;
            }
            NodeType::Prover => {
                Node::new_prover(node_ip, account, &self.peers, genesis, storage_mode).await?;
            }
            NodeType::Client => {
                Node::new_client(
                    node_ip,
                    Some(rest_ip),
                    self.rest_rps,
                    account,
                    &self.peers,
                    genesis,
                    None,
                    storage_mode,
                )
                .await?;
            }
        };

        // snarkos will close itself if this is not here...
        std::future::pending::<()>().await;

        Ok(())
    }
}
