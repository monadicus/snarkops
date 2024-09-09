use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

use aleo_std::StorageMode;
use anyhow::Result;
use checkpoint::{CheckpointManager, RetentionPolicy};
use clap::Args;
use rpc::RpcClient;
use snarkos_node::Node;
use snarkvm::{
    ledger::store::{
        helpers::rocksdb::{BlockDB, CommitteeDB},
        BlockStorage, CommitteeStorage,
    },
    prelude::Block,
    utilities::FromBytes,
};
use snops_common::state::{snarkos_status::SnarkOSStatus, NodeType};

use crate::{cli::ReloadHandler, Account, DbLedger, Key, Network};

mod metrics;
mod rpc;

/// A wrapper around the snarkos node run commands that provide additional
/// logging and configurability.
#[derive(Debug, Args)]
pub struct Runner<N: Network> {
    /// A path to the genesis block to initialize the ledger from.
    #[arg(short, long)]
    pub genesis: Option<PathBuf>,

    /// The ledger from which to view a block.
    #[arg(required = true, short, long, default_value = "./ledger")]
    pub ledger: PathBuf,

    /// The type of node to run: validator, prover, or client.
    #[arg(required = true, name = "type", short, long)]
    pub node_type: NodeType,

    #[clap(flatten)]
    pub key: Key<N>,

    /// Specify the IP(v4 or v6) address to bind to.
    #[clap(long = "bind", default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
    pub bind_addr: IpAddr,
    /// Specify the IP address and port for the node server.
    #[clap(long, default_value_t = 4130)]
    pub node: u16,
    /// Specify the IP address and port for the BFT.
    #[clap(long, default_value_t = 5000)]
    pub bft: u16,
    /// Specify the IP address and port for the REST server.
    #[clap(long, default_value_t = 3030)]
    pub rest: u16,
    /// Specify the port for the metrics server.
    #[clap(long, default_value_t = 9000)]
    pub metrics: u16,

    /// Specify the IP address and port of the peer(s) to connect to.
    #[clap(long, num_args = 1, value_delimiter = ',')]
    pub peers: Vec<SocketAddr>,
    /// Specify the IP address and port of the validator(s) to connect to.
    #[clap(long, num_args = 1, value_delimiter = ',')]
    pub validators: Vec<SocketAddr>,
    /// Specify the requests per second (RPS) rate limit per IP for the REST
    /// server.
    #[clap(long, default_value_t = 1000)]
    pub rest_rps: u32,

    /// The retention policy for the checkpoint manager. i.e. how often to
    /// create checkpoints.
    #[clap(long)]
    pub retention_policy: Option<RetentionPolicy>,

    /// When present, connects to an agent RPC server on the given port.
    #[clap(long)]
    pub agent_rpc_port: Option<u16>,
}

impl<N: Network> Runner<N> {
    pub fn parse(self, log_level_handler: ReloadHandler) -> Result<()> {
        if std::env::var("DEFAULT_RUNTIME").ok().is_some() {
            self.start_without_runtime(log_level_handler)
        } else {
            Self::runtime().block_on(async move { self.start(log_level_handler).await })
        }
    }

    #[tokio::main]
    pub async fn start_without_runtime(self, log_level_handler: ReloadHandler) -> Result<()> {
        self.start(log_level_handler).await
    }

    pub async fn start(self, log_level_handler: ReloadHandler) -> Result<()> {
        let agent = RpcClient::new(log_level_handler, self.agent_rpc_port);

        let res = self.start_inner(agent.to_owned()).await;

        if let Err(e) = &res {
            agent.status(SnarkOSStatus::Halted(Some(e.to_string())));
        }

        res
    }

    async fn start_inner(self, agent: RpcClient<N>) -> Result<()> {
        agent.status(SnarkOSStatus::Starting);

        let bind_addr = self.bind_addr;
        let node_ip = SocketAddr::new(bind_addr, self.node);
        let rest_ip = SocketAddr::new(bind_addr, self.rest);
        let bft_ip = SocketAddr::new(bind_addr, self.bft);
        let metrics_ip = SocketAddr::new(bind_addr, self.metrics);

        let account = Account::try_from(self.key.try_get()?)?;

        let genesis = if let Some(path) = self.genesis.as_ref() {
            Block::read_le(std::fs::File::open(path)?)?
        } else {
            Block::read_le(N::genesis_bytes())?
        };

        // conditionally create a checkpoint manager based on the presence
        // of a retention policy
        let mut manager = self
            .retention_policy
            .map(|p| CheckpointManager::load(self.ledger.clone(), p))
            .transpose()?;

        let storage_mode = StorageMode::Custom(self.ledger.clone());

        agent.status(SnarkOSStatus::LedgerLoading);
        if let Err(e) = DbLedger::<N>::load(genesis.clone(), storage_mode.clone()) {
            tracing::error!("aot failed to load ledger: {e:?}");
            agent.status(SnarkOSStatus::LedgerFailure(e.to_string()));
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
        let shutdown = Arc::new(AtomicBool::new(false));

        let _node = match self.node_type {
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
                    shutdown,
                )
                .await?
            }
            NodeType::Prover => {
                Node::new_prover(
                    node_ip,
                    account,
                    &self.peers,
                    genesis,
                    storage_mode.clone(),
                    shutdown,
                )
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
                    shutdown,
                )
                .await?
            }
        };

        // only monitor block updates if we have a checkpoint manager or agent status
        // API
        if manager.is_some() || agent.is_enabled() {
            // if we have a checkpoint manager, cull incompatible checkpoints
            if let Some(manager) = &mut manager {
                manager.cull_incompatible::<N>()?;
            }

            let committee = CommitteeDB::<N>::open(storage_mode.clone())?;
            let blocks = BlockDB::<N>::open(storage_mode.clone())?;
            // copy the block db to the agent's rpc server
            agent.set_block_db(blocks.clone());

            // check for height changes and poll the manager when a new block comes in
            let mut last_height = committee.current_height()?;

            // emit the initial block status
            agent.post_block(last_height, &blocks);

            tokio::spawn(async move {
                loop {
                    let Ok(height) = committee.current_height() else {
                        continue;
                    };

                    if last_height != height {
                        if last_height != 0 {
                            agent.status(SnarkOSStatus::Started);
                        }

                        last_height = height;

                        agent.post_block(height, &blocks);

                        if let Some(manager) = &mut manager {
                            if let Err(e) = manager.poll::<N>() {
                                tracing::error!("backup loop error: {e:?}");
                            }
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
