#[cfg(feature = "flame")]
use std::fs::File;
#[cfg(feature = "flame")]
use std::io::BufWriter;
use std::{io, path::PathBuf, thread};

use anyhow::Result;
#[cfg(any(feature = "clipages", feature = "mangen"))]
use clap::CommandFactory;
use clap::Parser;
#[cfg(feature = "node")]
use crossterm::tty::IsTty;
use reqwest::Url;
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{layer::SubscriberExt, reload, EnvFilter, Layer};

#[cfg(feature = "node")]
use crate::runner::Runner;
use crate::{
    accounts::GenAccounts, auth::AuthCommand, genesis::Genesis, ledger::Ledger,
    program::ProgramCommand, Network,
};

#[derive(Debug, Parser)]
#[clap(author = "MONADIC.US")]
pub struct Cli<N: Network> {
    #[arg(long)]
    pub enable_profiling: bool,

    /// The path to the log file.
    #[arg(long)]
    pub log: Option<PathBuf>,
    /// The verbosity level of the logs.
    #[arg(long, default_value_t = 4)]
    pub verbosity: u8,
    /// The optional loki url to send logs to.
    #[arg(long)]
    pub loki: Option<Url>,

    #[clap(subcommand)]
    pub command: Command<N>,
}

/// The different AOT commands.
#[derive(Debug, Parser)]
pub enum Command<N: Network> {
    Genesis(Genesis<N>),
    Accounts(GenAccounts),
    Ledger(Ledger<N>),
    #[cfg(feature = "node")]
    Run(Runner<N>),
    #[clap(subcommand)]
    Auth(AuthCommand<N>),
    #[clap(subcommand)]
    Program(ProgramCommand<N>),
    #[cfg(feature = "mangen")]
    Man(snops_common::mangen::Mangen),
    #[cfg(feature = "clipages")]
    Md(snops_common::clipages::Clipages),
}

pub trait Flushable {
    fn flush(&self);
}

impl Flushable for () {
    fn flush(&self) {}
}

#[cfg(feature = "flame")]
impl Flushable for tracing_flame::FlushGuard<BufWriter<File>> {
    fn flush(&self) {
        // Implementation for flushing if necessary
    }
}

#[cfg(feature = "flame")]
type FlameGuard = Box<dyn Flushable>;
#[cfg(not(feature = "flame"))]
type FlameGuard = ();

pub type ReloadHandler = reload::Handle<EnvFilter, tracing_subscriber::Registry>;

pub fn make_env_filter(level: Option<LevelFilter>, verbosity: Option<u8>) -> EnvFilter {
    let level = match level {
        Some(level) => level,
        None => match verbosity {
            Some(0) => LevelFilter::INFO,
            Some(1) => LevelFilter::DEBUG,
            Some(2..) => LevelFilter::TRACE,
            _ => LevelFilter::INFO,
        },
    };
    let verbosity = verbosity.unwrap_or(0);

    // Filter out undesirable logs. (unfortunately EnvFilter cannot be cloned)
    {
        let filter = tracing_subscriber::EnvFilter::builder()
            .with_env_var("AOT_LOG")
            .with_default_directive(level.into())
            .from_env_lossy()
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
    }
}

impl<N: Network> Cli<N> {
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
    pub fn init_logger(&self) -> (FlameGuard, Vec<WorkerGuard>, ReloadHandler) {
        let verbosity = self.verbosity;

        let (env_filter, reload_handler) =
            reload::Layer::new(make_env_filter(None, Some(verbosity)));

        let subscriber = tracing_subscriber::registry().with(env_filter);

        let mut layers = vec![];
        let mut guards = vec![];

        macro_rules! non_blocking_appender {
            ($name:ident = ( $args:expr )) => {
                let ($name, guard) = tracing_appender::non_blocking($args);
                guards.push(guard);
            };
        }

        if cfg!(not(feature = "flame")) && self.enable_profiling {
            // TODO should be an error
            panic!("Flame feature is not enabled");
        }

        #[cfg(feature = "flame")]
        let guard = if self.enable_profiling {
            let (flame_layer, guard) =
                tracing_flame::FlameLayer::with_file("./tracing.folded").unwrap();
            layers.push(flame_layer.boxed());
            Box::new(guard) as Box<dyn Flushable>
        } else {
            Box::new(())
        };

        #[cfg(not(feature = "flame"))]
        let guard = ();

        if let Some(logfile) = self.log.as_ref() {
            // Create the directories tree for a logfile if it doesn't exist.
            let logfile_dir = logfile
                .parent()
                .expect("Root directory passed as a logfile");
            if !logfile_dir.exists() {
                std::fs::create_dir_all(logfile_dir)
                .expect("Failed to create a directories: '{logfile_dir}', please check if user has permissions");
            }

            let file_appender = tracing_appender::rolling::daily(logfile_dir, logfile);
            non_blocking_appender!(log_writer = (file_appender));

            // Add layer redirecting logs to the file

            layers.push(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_thread_ids(true)
                    .with_writer(log_writer)
                    .boxed(),
            )
        };

        // Initialize tracing.
        // Add layer using LogWriter for stdout / terminal
        match self.command {
            #[cfg(feature = "node")]
            Command::Run(_) => {
                non_blocking_appender!(stdout = (io::stdout()));
                layers.push(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(io::stdout().is_tty())
                        .with_thread_ids(true)
                        .with_writer(stdout)
                        .boxed(),
                );
            }
            _ => {
                non_blocking_appender!(stderr = (io::stderr()));
                layers.push(tracing_subscriber::fmt::layer().with_writer(stderr).boxed());
            }
        }

        if let Some(loki) = &self.loki {
            let mut builder = tracing_loki::builder();

            let env_var = std::env::var("SNOPS_LOKI_LABELS").ok();
            let fields = match &env_var {
                Some(var) => var
                    .split(',')
                    .map(|item| item.split_once('=').unwrap_or((item, "")))
                    .collect(),
                None => vec![],
            };

            for (key, value) in fields {
                builder = builder.label(key, value).expect("bad loki label");
            }

            let (layer, task) = builder.build_url(loki.to_owned()).expect("bad loki url");
            thread::spawn(|| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let handle = rt.spawn(task);
                rt.block_on(handle).unwrap();
            });
            layers.push(layer.boxed());
        };

        tracing::subscriber::set_global_default(subscriber.with(layers)).unwrap();
        (guard, guards, reload_handler)
    }

    pub fn run(self) -> Result<()> {
        let (_guard, _guards, log_level_handler) = self.init_logger();

        match self.command {
            Command::Accounts(command) => command.parse::<N>(),
            Command::Genesis(command) => command.parse(),
            Command::Ledger(command) => command.parse(log_level_handler),
            #[cfg(feature = "node")]
            Command::Run(command) => command.parse(log_level_handler),
            Command::Auth(command) => command.parse(),
            Command::Program(command) => command.parse(),
            #[cfg(feature = "mangen")]
            Command::Man(mangen) => mangen.run(
                Cli::<N>::command(),
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_PKG_NAME"),
            ),
            #[cfg(feature = "clipages")]
            Command::Md(clipages) => clipages.run::<Cli<N>>(env!("CARGO_PKG_NAME")),
        }
    }
}
