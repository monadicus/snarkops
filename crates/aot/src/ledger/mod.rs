use std::{path::PathBuf, str::FromStr};

use anyhow::Result;
use clap::{Args, Subcommand};
use rand::{CryptoRng, Rng};
use snarkvm::{ledger::Block, utilities::FromBytes};

use self::checkpoint::CheckpointCommand;
use crate::{
    auth::execute::{execute_local, Execute},
    Network,
};

pub mod checkpoint;
pub mod hash;
pub mod init;
pub mod query;
pub mod truncate;
pub mod util;
pub mod view;

/// Commands for interacting with the ledger.
#[derive(Debug, Args)]
pub struct Ledger<N: Network> {
    #[arg(long)]
    pub enable_profiling: bool,

    /// A path to the genesis block to initialize the ledger from.
    #[arg(short, long, default_value = "./genesis.block")]
    pub genesis: Option<PathBuf>,

    /// The ledger from which to view a block.
    #[arg(required = true, short, long, default_value = "./ledger")]
    pub ledger: PathBuf,

    #[command(subcommand)]
    pub command: Commands<N>,
}

#[derive(Debug, Subcommand)]
pub enum Commands<N: Network> {
    Init(init::Init),
    #[clap(subcommand)]
    View(view::View<N>),
    #[clap(flatten)]
    Truncate(truncate::Truncate),
    Execute(Execute<N>),
    Query(query::LedgerQuery<N>),
    /// Hash the ledger.
    Hash,
    #[clap(subcommand)]
    Checkpoint(CheckpointCommand),
}

impl<N: Network> Ledger<N> {
    pub fn parse(self) -> Result<()> {
        // Common arguments
        let Ledger {
            genesis, ledger, ..
        } = self;

        let genesis_block = if let Some(path) = genesis {
            Block::read_le(std::fs::File::open(path)?)?
        } else {
            Block::read_le(N::genesis_bytes())?
        };

        match self.command {
            Commands::Init(init) => {
                let ledger = util::open_ledger(genesis_block, ledger)?;
                init.parse::<N>(&ledger)
            }

            Commands::View(view) => {
                let ledger = util::open_ledger(genesis_block, ledger)?;
                view.parse(&ledger)
            }

            Commands::Truncate(truncate) => truncate.parse::<N>(genesis_block, ledger),
            Commands::Execute(execute) => {
                let ledger = util::open_ledger(genesis_block, ledger)?;
                let tx = execute_local(
                    execute.auth.pick()?,
                    Some(&ledger),
                    None,
                    &mut rand::thread_rng(),
                )?;
                println!("{}", serde_json::to_string(&tx)?);
                Ok(())
            }

            Commands::Query(query) => {
                let ledger = util::open_ledger(genesis_block, ledger)?;
                query.parse(&ledger)
            }

            Commands::Hash => hash::hash_ledger(ledger),
            Commands::Checkpoint(command) => command.parse::<N>(genesis_block, ledger),
        }
    }
}
