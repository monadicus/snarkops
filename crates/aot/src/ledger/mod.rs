use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::Hasher,
    net::SocketAddr,
    ops::Deref,
    path::PathBuf,
    str::FromStr,
};

use anyhow::Result;
use clap::{Args, Subcommand};
use rand::{seq::SliceRandom, CryptoRng, Rng};
use serde::Serialize;
use snarkvm::{
    console::{network::MainnetV0, program::Network},
    ledger::store::helpers::rocksdb::{
        BFTMap, BlockMap, CommitteeMap, DeploymentMap, ExecutionMap, FeeMap, MapID, ProgramMap,
        TransactionMap, TransitionInputMap, TransitionMap, TransitionOutputMap, PREFIX_LEN,
    },
};
use tracing::warn;

use crate::{Address, PrivateKey};

pub mod add;
pub mod distribute;
pub mod init;
pub mod truncate;
pub mod tx;
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

    /// A truncate that breaks the ledger
    TruncateEvil {
        amount: u32,
    },

    /// At the moment this can be used as a diff tool for snarkos' rocksdb
    /// In the future, this should be able compare two rocksdbs and generate a patch
    Yoink {
        source: PathBuf,
    },
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

            Commands::Truncate(truncate) => truncate.parse(genesis, ledger),
            Commands::TruncateEvil { amount } => {
                let ledger = util::open_ledger(genesis, ledger)?;
                ledger.vm().block_store().remove_last_n(amount)?;
                Ok(())
            }
            Commands::Yoink { source } => {
                let source_db = rocks_open(source)?;
                // let dest_db = rocks_open(dest)?;

                let mut map: HashMap<u16, usize> = HashMap::new();
                let mut map_hashers: HashMap<u16, Box<dyn Hasher>> = HashMap::new();

                source_db
                    .iterator(rocksdb::IteratorMode::Start)
                    .flatten()
                    .for_each(|(key, value)| {
                        if key.len() < PREFIX_LEN {
                            return;
                        }
                        let (prefix, _) = key.split_at(PREFIX_LEN);
                        let mut network_id = [0u8; 2];
                        network_id.copy_from_slice(&prefix[0..2]);
                        let network_id = u16::from_le_bytes(network_id);
                        if network_id != MainnetV0::ID {
                            warn!("bad network id: {network_id}");
                            return;
                        }

                        let mut map_id = [0u8; 2];
                        map_id.copy_from_slice(&prefix[2..4]);
                        let map_id = u16::from_le_bytes(map_id);
                        // if map_id != u16::from(MapID::Program(ProgramMap::KeyValueID)) {
                        //     warn!("bad map id: {map_id}");
                        //     return false;
                        // }

                        *map.entry(map_id).or_default() += 1;
                        map_hashers
                            .entry(map_id)
                            .or_insert_with(|| Box::new(DefaultHasher::new()))
                            .write(&value);
                    });

                #[rustfmt::skip]
                let ids = vec![
                    // BFT
                    (MapID::BFT(BFTMap::Transmissions), "BFT(Transmissions)"),
                    (MapID::Block(BlockMap::StateRoot), "Block(StateRoot)"),
                    (MapID::Block(BlockMap::ReverseStateRoot), "Block(ReverseStateRoot)"),
                    (MapID::Block(BlockMap::ID), "Block(ID)"),
                    (MapID::Block(BlockMap::ReverseID), "Block(ReverseID)"),
                    (MapID::Block(BlockMap::Header), "Block(Header)"),
                    (MapID::Block(BlockMap::Authority), "Block(Authority)"),
                    (MapID::Block(BlockMap::Certificate), "Block(Certificate)"),
                    (MapID::Block(BlockMap::Ratifications), "Block(Ratifications)"),
                    (MapID::Block(BlockMap::Solutions), "Block(Solutions)"),
                    (MapID::Block(BlockMap::PuzzleCommitments), "Block(PuzzleCommitments)"),
                    (MapID::Block(BlockMap::AbortedSolutionIDs), "Block(AbortedSolutionIDs)"),
                    (MapID::Block(BlockMap::AbortedSolutionHeights), "Block(AbortedSolutionHeights)"),
                    (MapID::Block(BlockMap::Transactions), "Block(Transactions)"),
                    (MapID::Block(BlockMap::AbortedTransactionIDs), "Block(AbortedTransactionIDs)"),
                    (MapID::Block(BlockMap::RejectedOrAbortedTransactionID), "Block(RejectedOrAbortedTransactionID)"),
                    (MapID::Block(BlockMap::ConfirmedTransactions), "Block(ConfirmedTransactions)"),
                    (MapID::Block(BlockMap::RejectedDeploymentOrExecution), "Block(RejectedDeploymentOrExecution)"),
                    // Committee
                    (MapID::Committee(CommitteeMap::CurrentRound), "Committee(CurrentRound)"),
                    (MapID::Committee(CommitteeMap::RoundToHeight), "Committee(RoundToHeight)"),
                    (MapID::Committee(CommitteeMap::Committee), "Committee(Committee)"),
                    // Deployment
                    (MapID::Deployment(DeploymentMap::ID), "Deployment(ID)"),
                    (MapID::Deployment(DeploymentMap::Edition), "Deployment(Edition)"),
                    (MapID::Deployment(DeploymentMap::ReverseID), "Deployment(ReverseID)"),
                    (MapID::Deployment(DeploymentMap::Owner), "Deployment(Owner)"),
                    (MapID::Deployment(DeploymentMap::Program), "Deployment(Program)"),
                    (MapID::Deployment(DeploymentMap::VerifyingKey), "Deployment(VerifyingKey)"),
                    (MapID::Deployment(DeploymentMap::Certificate), "Deployment(Certificate)"),
                    // Execution
                    (MapID::Execution(ExecutionMap::ID), "Execution(ID)"),
                    (MapID::Execution(ExecutionMap::ReverseID), "Execution(ReverseID)"),
                    (MapID::Execution(ExecutionMap::Inclusion), "Execution(Inclusion)"),
                    // Fee
                    (MapID::Fee(FeeMap::Fee), "Fee(Fee)"),
                    (MapID::Fee(FeeMap::ReverseFee), "Fee(ReverseFee)"),
                    // Input
                    (MapID::TransitionInput(TransitionInputMap::ID), "TransitionInput(ID)"),
                    (MapID::TransitionInput(TransitionInputMap::ReverseID), "TransitionInput(ReverseID)"),
                    (MapID::TransitionInput(TransitionInputMap::Constant), "TransitionInput(Constant)"),
                    (MapID::TransitionInput(TransitionInputMap::Public), "TransitionInput(Public)"),
                    (MapID::TransitionInput(TransitionInputMap::Private), "TransitionInput(Private)"),
                    (MapID::TransitionInput(TransitionInputMap::Record), "TransitionInput(Record)"),
                    (MapID::TransitionInput(TransitionInputMap::RecordTag), "TransitionInput(RecordTag)"),
                    (MapID::TransitionInput(TransitionInputMap::ExternalRecord), "TransitionInput(ExternalRecord)"),
                    // Output
                    (MapID::TransitionOutput(TransitionOutputMap::ID), "TransitionOutput(ID)"),
                    (MapID::TransitionOutput(TransitionOutputMap::ReverseID), "TransitionOutput(ReverseID)"),
                    (MapID::TransitionOutput(TransitionOutputMap::Constant), "TransitionOutput(Constant)"),
                    (MapID::TransitionOutput(TransitionOutputMap::Public), "TransitionOutput(Public)"),
                    (MapID::TransitionOutput(TransitionOutputMap::Private), "TransitionOutput(Private)"),
                    (MapID::TransitionOutput(TransitionOutputMap::Record), "TransitionOutput(Record)"),
                    (MapID::TransitionOutput(TransitionOutputMap::RecordNonce), "TransitionOutput(RecordNonce)"),
                    (MapID::TransitionOutput(TransitionOutputMap::ExternalRecord), "TransitionOutput(ExternalRecord)"),
                    (MapID::TransitionOutput(TransitionOutputMap::Future), "TransitionOutput(Future)"),
                    // Transaction
                    (MapID::Transaction(TransactionMap::ID), "Transaction(ID)"),
                    // Transition
                    (MapID::Transition(TransitionMap::Locator), "Transition(Locator)"),
                    (MapID::Transition(TransitionMap::TPK), "Transition(TPK)"),
                    (MapID::Transition(TransitionMap::ReverseTPK), "Transition(ReverseTPK)"),
                    (MapID::Transition(TransitionMap::TCM), "Transition(TCM)"),
                    (MapID::Transition(TransitionMap::ReverseTCM), "Transition(ReverseTCM)"),
                    (MapID::Transition(TransitionMap::SCM), "Transition(SCM)"),
                    // Program
                    (MapID::Program(ProgramMap::ProgramID), "Program(ProgramID)"),
                    (MapID::Program(ProgramMap::KeyValueID),"Program(KeyValueID)"),
                ];

                for (id, name) in ids {
                    let count = map.get(&u16::from(id)).copied().unwrap_or_default();
                    let hash = map_hashers
                        .get(&u16::from(id))
                        .map(|hasher| hasher.finish())
                        .unwrap_or_default();
                    println!("{name}: {count} -- {hash:x}");
                }

                Ok(())
            }
        }
    }
}

fn rocks_open(dir: PathBuf) -> Result<rocksdb::DB> {
    let mut options = rocksdb::Options::default();
    options.set_compression_type(rocksdb::DBCompressionType::Lz4);

    // Register the prefix length.
    let prefix_extractor = rocksdb::SliceTransform::create_fixed_prefix(
        snarkvm::ledger::store::helpers::rocksdb::PREFIX_LEN,
    );
    options.set_prefix_extractor(prefix_extractor);
    options.increase_parallelism(2);
    options.set_max_background_jobs(4);
    options.create_if_missing(true);

    let db = rocksdb::DB::open(&options, dir)?;

    Ok(db)
}

fn _rocks_prefix(map_id: MapID) -> Vec<u8> {
    let mut context = MainnetV0::ID.to_le_bytes().to_vec();
    context.extend_from_slice(&(u16::from(map_id)).to_le_bytes());
    context
}

fn _rocks_key<K: Serialize>(map_id: MapID, key: K) -> Vec<u8> {
    let mut context = _rocks_prefix(map_id);
    bincode::serialize_into(&mut context, &key).unwrap();
    context
}
