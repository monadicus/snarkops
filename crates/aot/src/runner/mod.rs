use core::str::FromStr;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use aleo_std::StorageMode;
use anyhow::{bail, Result};
use checkpoint::{CheckpointManager, RetentionPolicy};
use clap::Args;
use snarkos_node::Node;
use snarkvm::{
    ledger::store::{helpers::rocksdb::CommitteeDB, CommitteeStorage},
    prelude::Block,
    utilities::FromBytes,
};
use snops_common::state::NodeType;

use crate::{Account, DbLedger, Network, PrivateKey};

mod metrics;

#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub struct Key<N: Network> {
    /// Specify the account private key of the node
    #[clap(long = "private-key")]
    pub private_key: Option<PrivateKey<N>>,
    /// Specify the account private key of the node
    #[clap(long = "private-key-file")]
    pub private_key_file: Option<PathBuf>,
}

impl<N: Network> Key<N> {
    pub fn try_get(self) -> Result<PrivateKey<N>> {
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

#[derive(Debug, Args)]
pub struct Runner<N: Network> {
    /// A path to the genesis block to initialize the ledger from.
    #[arg(required = true, short, long, default_value = "genesis.block")]
    pub genesis: PathBuf,

    /// The ledger from which to view a block.
    #[arg(required = true, short, long, default_value = "./ledger")]
    pub ledger: PathBuf,

    #[arg(required = true, name = "type", short, long)]
    pub node_type: NodeType,

    #[clap(flatten)]
    pub key: Key<N>,

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
    #[clap(
        long = "peers",
        default_value = "",
        num_args = 1,
        value_delimiter = ','
    )]
    pub peers: Vec<SocketAddr>,
    /// Specify the IP address and port of the validator(s) to connect to
    #[clap(
        long = "validators",
        default_value = "",
        num_args = 1,
        value_delimiter = ','
    )]
    pub validators: Vec<SocketAddr>,
    /// Specify the requests per second (RPS) rate limit per IP for the REST
    /// server
    #[clap(long = "rest-rps", default_value_t = 1000)]
    pub rest_rps: u32,

    #[clap(long = "retention-policy")]
    pub retention_policy: Option<RetentionPolicy>,
}

impl<N: Network> Runner<N> {
    pub fn parse(self) -> Result<()> {
        if std::env::var("DEFAULT_RUNTIME").ok().is_some() {
            self.start_without_runtime()
        } else {
            Self::runtime().block_on(async move { self.start().await })
        }
    }

    #[tokio::main]
    pub async fn start_without_runtime(self) -> Result<()> {
        self.start().await
    }

    pub async fn start(self) -> Result<()> {
        let bind_addr = self.bind_addr;
        let node_ip = SocketAddr::new(bind_addr, self.node);
        let rest_ip = SocketAddr::new(bind_addr, self.rest);
        let bft_ip = SocketAddr::new(bind_addr, self.bft);
        let metrics_ip = SocketAddr::new(bind_addr, self.metrics);

        let account = Account::try_from(self.key.try_get()?)?;

        let genesis = Block::from_bytes_le(&std::fs::read(&self.genesis)?)?;

        // conditionally create a checkpoint manager based on the presence
        // of a retention policy
        let mut manager = self
            .retention_policy
            .map(|p| CheckpointManager::load(self.ledger.clone(), p))
            .transpose()?;

        let storage_mode = StorageMode::Custom(self.ledger.clone());

        if let Err(e) = DbLedger::<N>::load(genesis.clone(), storage_mode.clone()) {
            tracing::error!("aot failed to load ledger: {e}");
            // L in binary = 01001100 = 76
            std::process::exit(76);
        }

        // slight alterations to the normal `metrics::initialize_metrics` because of
        // visibility issues
        {
            // Build the Prometheus exporter.
            if let Err(e) = metrics_exporter_prometheus::PrometheusBuilder::new()
                .with_http_listener(metrics_ip)
                .install()
            {
                tracing::error!("can't build the prometheus exporter: {e}");
            }

            // Register the snarkVM metrics.
            snarkvm::metrics::register_metrics();

            // Register the metrics so they exist on init.
            for name in metrics::GAUGE_NAMES {
                ::snarkos_node_metrics::register_gauge(name);
            }
            for name in metrics::COUNTER_NAMES {
                ::snarkos_node_metrics::register_counter(name);
            }
            for name in metrics::HISTOGRAM_NAMES {
                ::snarkos_node_metrics::register_histogram(name);
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
                    storage_mode.clone(),
                    false,
                    false,
                )
                .await?
            }
            NodeType::Prover => {
                Node::new_prover(node_ip, account, &self.peers, genesis, storage_mode.clone())
                    .await?
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
                    storage_mode.clone(),
                )
                .await?
            }
        };

        // if we have a checkpoint manager, start the backup loop
        if let Some(mut manager) = manager.take() {
            // cull incompatible checkpoints
            manager.cull_incompatible::<N>()?;

            let committee = CommitteeDB::<N>::open(storage_mode.clone())?;

            // check for height changes and poll the manager when a new block comes in
            let mut last_height = committee.current_height()?;
            tokio::spawn(async move {
                loop {
                    let Ok(height) = committee.current_height() else {
                        continue;
                    };

                    if last_height != height {
                        last_height = height;

                        if let Err(e) = manager.poll::<N>() {
                            tracing::error!("backup loop error: {e:?}");
                        }
                    }

                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            });
        }

        // snarkos will close itself if this is not here...
        std::future::pending::<()>().await;

        Ok(())
    }

    /// Returns a runtime for the node.
    pub fn runtime() -> tokio::runtime::Runtime {
        // Retrieve the number of cores.
        let num_cores = num_cpus::get();

        // Initialize the number of tokio worker threads, max tokio blocking threads,
        // and rayon cores. Note: We intentionally set the number of tokio
        // worker threads and number of rayon cores to be more than the number
        // of physical cores, because the node is expected to be I/O-bound.
        let (num_tokio_worker_threads, max_tokio_blocking_threads, num_rayon_cores_global) =
            (2 * num_cores, 512, num_cores);

        // Initialize the parallelization parameters.
        rayon::ThreadPoolBuilder::new()
            .stack_size(8 * 1024 * 1024)
            .num_threads(num_rayon_cores_global)
            .build_global()
            .unwrap();

        // Initialize the runtime configuration.
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(8 * 1024 * 1024)
            .worker_threads(num_tokio_worker_threads)
            .max_blocking_threads(max_tokio_blocking_threads)
            .build()
            .expect("Failed to initialize a runtime for the router")
    }
}
