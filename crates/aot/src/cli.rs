use anyhow::Result;
use clap::Parser;
use tracing_subscriber::layer::SubscriberExt;

use crate::{genesis::Genesis, ledger::Ledger};

#[derive(Debug, Parser)]
#[clap(name = "snarkOS AoT", author = "MONADIC.US")]
pub struct Cli {
    #[arg(long)]
    pub enable_profiling: bool,

    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Parser)]
pub enum Command {
    #[clap(name = "genesis")]
    Genesis(Genesis),
    #[clap(name = "ledger")]
    Ledger(Ledger),
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let fmt_layer = tracing_subscriber::fmt::Layer::default().with_writer(std::io::stderr);

        let (flame_layer, _guard) = if self.enable_profiling {
            let (flame_layer, guard) =
                tracing_flame::FlameLayer::with_file("./tracing.folded").unwrap();
            (Some(flame_layer), Some(guard))
        } else {
            (None, None)
        };

        let subscriber = tracing_subscriber::registry::Registry::default()
            .with(fmt_layer)
            .with(flame_layer);

        tracing::subscriber::set_global_default(subscriber).unwrap();

        match self.command {
            Command::Genesis(command) => command.parse(),
            Command::Ledger(command) => command.parse(),
        }
    }
}
