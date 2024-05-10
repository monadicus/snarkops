use std::{net::SocketAddr, ops::Deref, path::PathBuf, str::FromStr};

use anyhow::Result;
use clap::{Args, Subcommand};
use rand::{CryptoRng, Rng};
use tracing::warn;

use self::checkpoint::CheckpointCommand;
use crate::{authorized::Execute, Address, PrivateKey};

pub mod checkpoint;
pub mod distribute;
pub mod hash;
pub mod init;
pub mod query;
pub mod truncate;
pub mod util;
pub mod view;

#[derive(Debug, Args)]
pub struct Ledger {
    #[arg(long)]
    pub enable_profiling: bool,

    /// A path to the genesis block to initialize the ledger from.
    #[arg(required = true, short, long, default_value = "./genesis.block")]
    pub genesis: PathBuf,

    /// The ledger from which to view a block.
    #[arg(required = true, short, long, default_value = "./ledger")]
    pub ledger: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

// Helper macro for making clap args that are comma-separated
macro_rules! comma_separated {
    { $name:ident ( $item:ty ) ; } => {
        #[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
        pub struct $name(Vec<$item>);

        impl FromStr for $name {
					type Err = anyhow::Error;

					fn from_str(s: &str) -> Result<Self, Self::Err> {
						if s.is_empty() {
							return Ok(Self(Vec::new()));
						}

						Ok(Self(s.split(',')
										 .map(|i| <$item>::from_str(i))
										 .collect::<Result<Vec<_>, <$item as FromStr>::Err>>()
										 .map_err(anyhow::Error::from)?))
					}
			}

        impl Deref for $name {
            type Target = Vec<$item>;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };

    // Tail recursion for extra types
    { $name:ident ( $item:ty ) ; $( $name2:ident ( $item2:ty ) ; )+ } => {
        comma_separated! { $name ( $item ) ; }
        comma_separated! { $($name2 ( $item2 ) ;)+ }
    };
}

comma_separated! {
    PrivateKeys(PrivateKey);
    Accounts(Address);
    Addrs(SocketAddr);
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init(init::Init),
    #[clap(subcommand)]
    View(view::View),
    Distribute(distribute::Distribute),
    #[clap(flatten)]
    Truncate(truncate::Truncate),
    Execute(Execute),
    Query(query::LedgerQuery),
    Hash,
    #[clap(subcommand)]
    Checkpoint(CheckpointCommand),
}

impl Ledger {
    pub fn parse(self) -> Result<()> {
        // Common arguments
        let Ledger {
            genesis, ledger, ..
        } = self;

        match self.command {
            Commands::Init(init) => {
                let ledger = util::open_ledger(genesis, ledger)?;
                init.parse(&ledger)
            }

            Commands::View(view) => {
                let ledger = util::open_ledger(genesis, ledger)?;
                view.parse(&ledger)
            }

            Commands::Distribute(distribute) => {
                let ledger = util::open_ledger(genesis, ledger)?;
                distribute.parse(&ledger)
            }

            Commands::Truncate(truncate) => truncate.parse(genesis, ledger),
            Commands::Execute(execute) => {
                let ledger = util::open_ledger(genesis, ledger)?;
                let tx = execute.authorization.execute_local(
                    Some(&ledger),
                    &mut rand::thread_rng(),
                    None,
                    None,
                )?;
                println!("{}", serde_json::to_string(&tx)?);
                Ok(())
            }

            Commands::Query(query) => {
                let ledger = util::open_ledger(genesis, ledger)?;
                query.parse(&ledger)
            }

            Commands::Hash => hash::hash_ledger(ledger),
            Commands::Checkpoint(command) => command.parse(genesis, ledger),
        }
    }
}
