use anyhow::Result;
use clap::Parser;
use crossterm::tty::IsTty;
use std::{
    fs::File,
    io::{self, BufWriter},
    path::PathBuf,
};
use tracing_flame::FlushGuard;
use tracing_subscriber::{layer::SubscriberExt, Layer};

#[cfg(feature = "node")]
use crate::runner::Runner;
use crate::{genesis::Genesis, ledger::Ledger};

#[derive(Debug, Parser)]
#[clap(name = "snarkOS AoT", author = "MONADIC.US")]
pub struct Cli {
    #[arg(long)]
    pub enable_profiling: bool,

    #[arg(long)]
    pub log: Option<PathBuf>,
    #[arg(long, default_value_t = 4)]
    pub verbosity: u8,

    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Parser)]
pub enum Command {
    Genesis(Genesis),
    Ledger(Ledger),
    #[cfg(feature = "node")]
    Run(Runner),
}

impl Cli {
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
    pub fn init_logger(&self) -> Option<FlushGuard<BufWriter<File>>> {
        let verbosity = self.verbosity;

        match verbosity {
            0 => std::env::set_var("RUST_LOG", "info"),
            1 => std::env::set_var("RUST_LOG", "debug"),
            2.. => std::env::set_var("RUST_LOG", "trace"),
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

        let mut layers = vec![];

        let guard = if self.enable_profiling {
            let (flame_layer, guard) =
                tracing_flame::FlameLayer::with_file("./tracing.folded").unwrap();
            layers.push(flame_layer.boxed());
            Some(guard)
        } else {
            None
        };

        if let Some(logfile) = &self.log {
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

            // Add layer redirecting logs to the file
            layers.push(
                tracing_subscriber::fmt::Layer::default()
                    .with_ansi(false)
                    .with_writer(logfile)
                    .with_filter(filter2)
                    .boxed(),
            );
        }

        // Initialize tracing.
        // Add layer using LogWriter for stdout / terminal
        if matches!(self.command, Command::Run(_)) {
            layers.push(
                tracing_subscriber::fmt::Layer::default()
                    .with_ansi(io::stdout().is_tty())
                    .with_filter(filter)
                    .boxed(),
            );
        } else {
            layers.push(
                tracing_subscriber::fmt::Layer::default()
                    .with_writer(io::stderr)
                    .boxed(),
            );
        }

        let subscriber = tracing_subscriber::registry::Registry::default().with(layers);
        tracing::subscriber::set_global_default(subscriber).unwrap();
        guard
    }

    pub fn run(self) -> Result<()> {
        self.init_logger();

        match self.command {
            Command::Genesis(command) => command.parse(),
            Command::Ledger(command) => command.parse(),
            #[cfg(feature = "node")]
            Command::Run(command) => command.parse(),
        }
    }
}
