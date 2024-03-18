use std::{
    fs::File,
    io,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use aleo_std::StorageMode;
use anyhow::Result;
use clap::Args;
use crossterm::tty::IsTty;
use serde::{Deserialize, Serialize};
use serde_clap_deserialize::serde_clap_default;
use snarkos_node::Node;
use snarkvm::{prelude::Block, utilities::FromBytes};
use snot_common::state::NodeType;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

use crate::{ledger::Addrs, Account, PrivateKey};

#[serde_clap_default]
#[derive(Debug, Args, Serialize, Deserialize)]
pub struct Runner {
    /// A path to the genesis block to initialize the ledger from.
    #[arg(required = true, short, long)]
    #[serde_clap_default(PathBuf::from("genesis.block"))]
    pub genesis: PathBuf,

    /// The ledger from which to view a block.
    #[arg(required = true, short, long)]
    #[serde_clap_default(PathBuf::from("./ledger"))]
    pub ledger: PathBuf,

    #[arg(required = true, name = "type", short, long)]
    pub node_type: NodeType,

    /// Specify the account private key of the node
    #[clap(long = "private-key")]
    pub private_key: PrivateKey,

    /// A path to the log file
    #[clap(long = "log")]
    #[serde_clap_default(PathBuf::from("snarkos.log"))]
    pub log: PathBuf,

    /// Specify the IP address and port for the node server
    #[clap(long = "node")]
    #[serde_clap_default(4130)]
    pub node: u16,
    /// Specify the IP address and port for the BFT
    #[clap(long = "bft")]
    #[serde_clap_default(Some(5000))]
    pub bft: Option<u16>,
    /// Specify the IP address and port of the peer(s) to connect to
    #[clap(long = "peers")]
    #[serde_clap_default(Default::default())]
    pub peers: Addrs,
    /// Specify the IP address and port of the validator(s) to connect to
    #[clap(long = "validators")]
    #[serde_clap_default(Default::default())]
    pub validators: Addrs,
    /// Specify the IP address and port for the REST server
    #[clap(long = "rest")]
    #[serde_clap_default(Some(3030))]
    pub rest: Option<u16>,
    /// Specify the requests per second (RPS) rate limit per IP for the REST
    /// server
    #[clap(long = "rest-rps")]
    #[serde_clap_default(1000)]
    pub rest_rps: u32,
    // TODO: --verbosity - see snarkos-cli::helpers::initialize_logger
}

impl Runner {
    /// Initializes the logger.
    ///
    /// ```ignore
    /// 0 => info
    /// 1 => info, debug
    /// 2 => info, debug, trace, snarkos_node_sync=trace
    /// 3 => info, debug, trace, snarkos_node_bft=trace
    /// 4 => info, debug, trace, snarkos_node_bft::gateway=trace
    /// 5 => info, debug, trace, snarkos_node_router=trace
    /// 6 => info, debug, trace, snarkos_node_tcp=trace
    /// ```
    pub fn init_logger(&self) {
        let verbosity = 4;
        let logfile = &self.log;

        match verbosity {
            0 => std::env::set_var("RUST_LOG", "info"),
            1 => std::env::set_var("RUST_LOG", "debug"),
            2.. => std::env::set_var("RUST_LOG", "trace"),
            _ => {}
        };

        // Filter out undesirable logs. (unfortunately EnvFilter cannot be cloned)
        let [filter, filter2] = std::array::from_fn(|_| {
            let filter = tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("mio=off".parse().unwrap())
                .add_directive("tokio_util=off".parse().unwrap())
                .add_directive("hyper=off".parse().unwrap())
                .add_directive("reqwest=off".parse().unwrap())
                .add_directive("want=off".parse().unwrap())
                .add_directive("warp=off".parse().unwrap());

            let filter = if verbosity >= 2 {
                filter.add_directive("snarkos_node_sync=trace".parse().unwrap())
            } else {
                filter.add_directive("snarkos_node_sync=debug".parse().unwrap())
            };

            let filter = if verbosity >= 3 {
                filter
                    .add_directive("snarkos_node_bft=trace".parse().unwrap())
                    .add_directive("snarkos_node_bft::gateway=debug".parse().unwrap())
            } else {
                filter.add_directive("snarkos_node_bft=debug".parse().unwrap())
            };

            let filter = if verbosity >= 4 {
                filter.add_directive("snarkos_node_bft::gateway=trace".parse().unwrap())
            } else {
                filter.add_directive("snarkos_node_bft::gateway=debug".parse().unwrap())
            };

            let filter = if verbosity >= 5 {
                filter.add_directive("snarkos_node_router=trace".parse().unwrap())
            } else {
                filter.add_directive("snarkos_node_router=debug".parse().unwrap())
            };

            if verbosity >= 6 {
                filter.add_directive("snarkos_node_tcp=trace".parse().unwrap())
            } else {
                filter.add_directive("snarkos_node_tcp=off".parse().unwrap())
            }
        });

        // Create the directories tree for a logfile if it doesn't exist.
        let logfile_dir = logfile
            .parent()
            .expect("Root directory passed as a logfile");
        if !logfile_dir.exists() {
            std::fs::create_dir_all(logfile_dir)
            .expect("Failed to create a directories: '{logfile_dir}', please check if user has permissions");
        }
        // Create a file to write logs to.
        // TODO: log rotation
        let logfile = File::options()
            .append(true)
            .create(true)
            .open(logfile)
            .expect("Failed to open the file for writing logs");

        // Initialize tracing.
        let _ = tracing_subscriber::registry()
            .with(
                // Add layer using LogWriter for stdout / terminal
                tracing_subscriber::fmt::Layer::default()
                    .with_ansi(io::stdout().is_tty())
                    .with_target(verbosity > 2)
                    .with_filter(filter),
            )
            .with(
                // Add layer redirecting logs to the file
                tracing_subscriber::fmt::Layer::default()
                    .with_ansi(false)
                    .with_writer(logfile)
                    .with_target(verbosity > 2)
                    .with_filter(filter2),
            )
            .try_init();
    }

    #[tokio::main]
    pub async fn parse(self) -> Result<()> {
        self.init_logger();

        let node_ip = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), self.node);
        let rest_ip = self
            .rest
            .map(|port| SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port));
        let bft_ip = self
            .bft
            .map(|port| SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port));

        let account = Account::try_from(self.private_key)?;

        let genesis = Block::from_bytes_le(&std::fs::read(&self.genesis)?)?;
        let storage_mode = StorageMode::Custom(self.ledger);

        // snarkos_node::metrics::initialize_metrics();

        match self.node_type {
            NodeType::Validator => {
                Node::new_validator(
                    node_ip,
                    bft_ip,
                    rest_ip,
                    self.rest_rps,
                    account,
                    &self.peers,
                    &self.validators,
                    genesis,
                    None,
                    storage_mode,
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
                    rest_ip,
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
/*



*/
