use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
};

use aleo_std::StorageMode;
use anyhow::{Context, Result, anyhow};
use clap::Args;
use rpc::RpcClient;
use snarkos_node::{
    Node,
    bft::helpers::{ProposalCache, proposal_cache_path},
};
use snarkos_utilities::{NodeDataDir, SignalHandler};
use snarkvm::{
    ledger::store::{
        BlockStorage, CommitteeStorage,
        helpers::rocksdb::{BlockDB, CommitteeDB},
    },
    prelude::Block,
    utilities::FromBytes,
};
use snops_checkpoint::{CheckpointManager, RetentionPolicy};
use snops_common::state::{NodeType, snarkos_status::SnarkOSStatus};
use tracing::info;

use crate::{Account, Address, DbLedger, Key, Network, cli::ReloadHandler};

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
    /// The path to the node data directory, defaults to a sibling to the
    /// ledger called `node-data`.
    #[arg(long)]
    pub node_data: Option<PathBuf>,

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

        let account = Account::try_from(
            self.key
                .try_get()
                .map_err(|e| e.context("obtain private key"))?,
        )?;

        let genesis =
            if let Some(path) = self.genesis.as_ref() {
                Block::read_le(std::fs::File::open(path).map_err(|e| {
                    anyhow!(e).context(format!("open genesis file {}", path.display()))
                })?)
                .map_err(|e| anyhow!(e).context("parse genesis block from file"))?
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
        let node_data_dir = NodeDataDir::new(self.node_data.unwrap_or_else(|| {
            // Append the `node-data` directory to the ledger path.
            let mut ledger_dir = self.ledger.clone();
            ledger_dir.pop();
            ledger_dir.push("node-data");
            ledger_dir
        }));

        // Ensure node data dir exists
        fs::create_dir_all(node_data_dir.path()).with_context(|| {
            format!(
                "create node data directory {}",
                node_data_dir.path().display()
            )
        })?;

        Self::check_for_old_storage_format(&self.ledger, &node_data_dir)?;

        agent.status(SnarkOSStatus::LedgerLoading);

        {
            let genesis = genesis.clone();
            let storage_mode = storage_mode.clone();
            // The ledger loading must be blocking to avoid some race conditions with the
            // node startup
            // This is based on the `spawn_blocking!` macro in snarkos' codebase
            if let Err(e) =
                tokio::task::spawn_blocking(move || DbLedger::<N>::load(genesis, storage_mode))
                    .await
            {
                tracing::error!("aot failed to load ledger: {e:?}");
                agent.status(SnarkOSStatus::LedgerFailure(e.to_string()));
                // L in binary = 01001100 = 76
                std::process::exit(76);
            }
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
        let shutdown = SignalHandler::new();

        let node = match self.node_type {
            NodeType::Validator => {
                Self::check_proposal_cache(account.address(), &node_data_dir);
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
                    node_data_dir,
                    false,
                    false,
                    None,
                    Arc::clone(&shutdown),
                )
                .await
                .map_err(|e| e.context("create validator"))?
            }
            NodeType::Prover => Node::new_prover(
                node_ip,
                account,
                &self.peers,
                genesis,
                node_data_dir,
                false,
                None,
                Arc::clone(&shutdown),
            )
            .await
            .map_err(|e| e.context("create prover"))?,
            NodeType::Client => Node::new_client(
                node_ip,
                Some(rest_ip),
                self.rest_rps,
                account,
                &self.peers,
                genesis,
                None,
                storage_mode.clone(),
                node_data_dir,
                false,
                None,
                Arc::clone(&shutdown),
            )
            .await
            .map_err(|e| e.context("create client"))?,
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

        // Wait for graceful shutdown
        node.wait_for_signals(&shutdown).await;

        Ok(())
    }

    /// Check the proposal cache for this address and remove it if it is
    /// invalid.
    fn check_proposal_cache(addr: Address<N>, node_data_dir: &NodeDataDir) {
        let proposal_cache_path = proposal_cache_path(node_data_dir);
        if !proposal_cache_path.exists() {
            return;
        }

        let Err(e) = ProposalCache::<N>::load(addr, node_data_dir) else {
            return;
        };

        tracing::error!("failed to load proposal cache: {e}");
        if let Err(e) = std::fs::remove_file(&proposal_cache_path) {
            tracing::error!("failed to remove proposal cache: {e}");
        }
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

    /// Check if the node is still using the old storage format,
    /// in which case we print an error and exit.
    /// We detect this by checking if
    /// - a peer-cache file exists inside the ledger directory,
    /// - a current-proposal-cache file exists at the parent directory of the
    ///   ledger directory
    /// - a jwt_secret_*.txt file exists at the parent directory of the ledger
    ///   directory
    fn check_for_old_storage_format(ledger_dir: &Path, node_data_dir: &NodeDataDir) -> Result<()> {
        use snarkos_utilities::node_data;

        // Determine the old paths used for node configuration files.
        let old_proposal_cache_path =
            ledger_dir.join(node_data::legacy_current_proposal_cache_file(N::ID, None));

        if old_proposal_cache_path.exists() {
            let new_proposal_cache_path = node_data_dir.current_proposal_cache_path();
            info!(
                "Migrating node data file \"{old_proposal_cache_path:?}\" to \"{new_proposal_cache_path:?}\""
            );
            fs::rename(old_proposal_cache_path, new_proposal_cache_path)
                .with_context(|| "Failed to migrate node data file")?;
        }
        Ok(())
    }
}
