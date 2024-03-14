use std::{ops::Deref, path::PathBuf, str::FromStr};

use anyhow::{bail, ensure, Result};
use clap::{Args, Subcommand};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressIterator};
use rand::{seq::SliceRandom, thread_rng, CryptoRng, Rng, SeedableRng};
use rand_chacha::ChaChaRng;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::Deserialize;
use snarkvm::{circuit::AleoV0, ledger::Transaction};
use tracing::{span, Level};
use tracing_subscriber::layer::SubscriberExt;

use self::util::{add_transaction_blocks, make_transaction_proof};
use crate::types::*;

mod add;
mod distribute;
mod init;
mod truncate;
mod tx;
mod util;
mod view;

#[derive(Debug, Args)]
pub struct Ledger {
    #[arg(long)]
    pub enable_profiling: bool,

    /// A path to the genesis block to initialize the ledger from.
    #[arg(required = true, short, long, default_value = "./genesis.block")]
    genesis: PathBuf,
    /// The ledger from which to view a block.
    #[arg(required = true, short, long, default_value = "./ledger")]
    ledger: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

// Helper macro for making clap args that are comma-separated
macro_rules! comma_separated {
    { $name:ident ( $item:ty ) ; } => {
        #[derive(Debug, Clone)]
        pub struct $name(Vec<$item>);

        impl FromStr for $name {
            type Err = <$item as FromStr>::Err;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.split(',').map(<$item>::from_str).collect::<Result<Vec<_>>>()?))
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
}

impl PrivateKeys {
    /// Returns a random 2 or 3 private keys.
    fn random_accounts<R: Rng + CryptoRng>(&self, rng: &mut R) -> Vec<PrivateKey> {
        let num = rng.gen_range(2..=3);
        let chosen = self.0.choose_multiple(rng, num);

        chosen.copied().collect()
    }
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init(init::Init),
    #[clap(subcommand)]
    Tx(tx::Tx),
    #[clap(subcommand)]
    Add(add::Add),
    #[clap(subcommand)]
    View(view::View),
    Distribute(distribute::Distribute),
    Truncate(truncate::Truncate),
}

impl Ledger {
    pub fn parse(self) -> Result<()> {
        // Initialize logging.
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

        // Common arguments
        let Ledger {
            genesis, ledger, ..
        } = self;

        match self.command {
            Commands::Init(init) => {
                let ledger = util::open_ledger(genesis, ledger)?;
                init.parse(&ledger)
            }

            Commands::Tx(tx) => {
                // load the ledger into memory
                // the secret sauce is `ConsensusMemory`, which tells snarkvm to keep the ledger
                // in memory only
                let ledger = util::open_ledger(genesis, ledger)?;
                tx.parse(&ledger)
            }

            Commands::Add(add) => {
                let ledger = util::open_ledger(genesis, ledger)?;
                add.parse(&ledger)
            }

            Commands::View(view) => {
                let ledger = util::open_ledger(genesis, ledger)?;
                view.parse(&ledger)
            }

            Commands::Distribute(distribute) => {
                let ledger = util::open_ledger(genesis, ledger)?;
                distribute.parse(&ledger)
            }

            Commands::Truncate(truncate) => {
                let ledger = util::open_ledger(genesis, ledger)?;
                truncate.parse(&ledger)
            }
        }
    }
}
