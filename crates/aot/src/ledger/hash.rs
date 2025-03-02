use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::Hasher,
    path::PathBuf,
};

use anyhow::Result;
use indexmap::IndexSet;
use serde::Serialize;
use snarkvm::{
    console::{network::MainnetV0, program::Network, types::Field},
    ledger::{
        narwhal::{Transmission, TransmissionID},
        store::helpers::rocksdb::{
            BFTMap, BlockMap, CommitteeMap, DeploymentMap, ExecutionMap, FeeMap, MapID, PREFIX_LEN,
            ProgramMap, TransactionMap, TransitionInputMap, TransitionMap, TransitionOutputMap,
        },
    },
};
use tracing::warn;

pub fn hash_ledger(ledger: PathBuf) -> Result<()> {
    let source_db = rocks_open(ledger)?;
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
            let (prefix, rest) = key.split_at(PREFIX_LEN);
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

            *map.entry(map_id).or_default() += 1;

            if map_id == BFTMap::Transmissions as u16 {
                type N = MainnetV0;
                match (
                    bincode::deserialize::<TransmissionID<N>>(rest),
                    bincode::deserialize::<(Transmission<N>, IndexSet<Field<N>>)>(&value),
                ) {
                    (Ok(k), Ok(v)) => {
                        println!("transmission {k} = {v:?}");
                    }
                    (a, b) => {
                        warn!("failed to deserialize transmission: {a:?} {b:?}");
                    }
                }
            }

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

/*

Notes for how the documents are formed

CONSENSUS STORAGE:
    FINALIZE STORAGE
        COMITTEE STORAGE
        - program docs

    BLOCK STORAGE
        - block docs
        TRANSACTION STORAGE
        TRANSITION STORAGE
    TRANSACTION STORAGE
        - id
        DEPLOYMENT STORAGE
        EXECUTION STORAGE
        FEE STORAGE
        TRANSITION STORAGE
    TRANSITION STORAGE
        - transition docs
        INPUT STORAGE
        OUTPUT STORAGE


KEY TRANSMISSION ID (MapID::BFT(BFTMap::Transmissions), "BFT(Transmissions)"),


HEIGHT KEY -> STATE ROOT (MapID::Block(BlockMap::StateRoot), "Block(StateRoot)"),
STATE ROOT KEY -> HEIGHT (MapID::Block(BlockMap::ReverseStateRoot), "Block(ReverseStateRoot)"),
HEIGHT KEY -> HASH (MapID::Block(BlockMap::ID), "Block(ID)"),
HASH KEY -> HEIGHT (MapID::Block(BlockMap::ReverseID), "Block(ReverseID)"),
HASH KEY (MapID::Block(BlockMap::Header), "Block(Header)"),
HASH KEY (MapID::Block(BlockMap::Authority), "Block(Authority)"),
CERTIFICATE KEY -> (block height, round round) (MapID::Block(BlockMap::Certificate), "Block(Certificate)"),
HASH KEY (MapID::Block(BlockMap::Ratifications), "Block(Ratifications)"),
HASH KEY (MapID::Block(BlockMap::Solutions), "Block(Solutions)"),
PUZZLE COMMITMENT -> HEIGHT (MapID::Block(BlockMap::PuzzleCommitments), "Block(PuzzleCommitments)"),
HASH -> COMMITMENT (MapID::Block(BlockMap::AbortedSolutionIDs), "Block(AbortedSolutionIDs)"),
PUZZLE COMMITMENT -> HEIGHT (MapID::Block(BlockMap::AbortedSolutionHeights), "Block(AbortedSolutionHeights)"),
HASH -> TRANSACTION ID (MapID::Block(BlockMap::Transactions), "Block(Transactions)"),
HASH -> TRANSACTION ID (MapID::Block(BlockMap::AbortedTransactionIDs), "Block(AbortedTransactionIDs)"),
ID -> HASH (MapID::Block(BlockMap::RejectedOrAbortedTransactionID), "Block(RejectedOrAbortedTransactionID)"),
TRANSACTION ID -> (hash, type) (MapID::Block(BlockMap::ConfirmedTransactions), "Block(ConfirmedTransactions)"),
FIELD -> REJECTED? (MapID::Block(BlockMap::RejectedDeploymentOrExecution), "Block(RejectedDeploymentOrExecution)"),



// Committee
LITERALLY "0" KEY -> ROUND HEIGHT (MapID::Committee(CommitteeMap::CurrentRound), "Committee(CurrentRound)"),
ROUND HEIGHT -> BLOCK HEIGHT  (MapID::Committee(CommitteeMap::RoundToHeight), "Committee(RoundToHeight)"),
HEIGHT KEY -> COMMITEE (MapID::Committee(CommitteeMap::Committee), "Committee(Committee)"),

// Deployment
INSERT TRANSACTION ID -> PROGRAM ID (MapID::Deployment(DeploymentMap::ID), "Deployment(ID)"),

PROGRAM ID KEY (MapID::Deployment(DeploymentMap::Edition), "Deployment(Edition)"),
PROGRAM ID KEY (MapID::Deployment(DeploymentMap::ReverseID), "Deployment(ReverseID)"),
PROGRAM ID KEY (MapID::Deployment(DeploymentMap::Owner), "Deployment(Owner)"),
PROGRAM ID KEY (MapID::Deployment(DeploymentMap::Program), "Deployment(Program)"),
PROGRAM ID KEY (MapID::Deployment(DeploymentMap::VerifyingKey), "Deployment(VerifyingKey)"),
PROGRAM ID KEY (MapID::Deployment(DeploymentMap::Certificate), "Deployment(Certificate)"),

// Execution
TRANSACTION ID KEY (MapID::Execution(ExecutionMap::ID), "Execution(ID)"),
TRANSACTION ID KEY (MapID::Execution(ExecutionMap::ReverseID), "Execution(ReverseID)"),
TRANSACTION ID KEY (MapID::Execution(ExecutionMap::Inclusion), "Execution(Inclusion)"),

// Fee
TRANSACTION ID KEY (MapID::Fee(FeeMap::Fee), "Fee(Fee)"),
TRANSITION ID KEY (MapID::Fee(FeeMap::ReverseFee), "Fee(ReverseFee)"),

// Input
TRANSITION KEY, MAPS -> INPUT IDS (MapID::TransitionInput(TransitionInputMap::ID), "TransitionInput(ID)"),

INPUT ID MAP -> TRANSITION ID (MapID::TransitionInput(TransitionInputMap::ReverseID), "TransitionInput(ReverseID)"),

INPUT ID KEY (MapID::TransitionInput(TransitionInputMap::Constant), "TransitionInput(Constant)"),
INPUT ID KEY (MapID::TransitionInput(TransitionInputMap::Public), "TransitionInput(Public)"),
INPUT ID KEY (MapID::TransitionInput(TransitionInputMap::Private), "TransitionInput(Private)"),
INPUT ID KEY (MapID::TransitionInput(TransitionInputMap::ExternalRecord), "TransitionInput(ExternalRecord)"),
SERIAL KEY (MapID::TransitionInput(TransitionInputMap::Record), "TransitionInput(Record)"),
TAG KEY (MapID::TransitionInput(TransitionInputMap::RecordTag), "TransitionInput(RecordTag)"),

// Output
TRANSITION ID KEY -> VEC<FIELD> (MapID::TransitionOutput(TransitionOutputMap::ID), "TransitionOutput(ID)"),
FIELD KEY -> TRANSITION ID (MapID::TransitionOutput(TransitionOutputMap::ReverseID), "TransitionOutput(ReverseID)"),
FIELD KEY (MapID::TransitionOutput(TransitionOutputMap::Constant), "TransitionOutput(Constant)"),
FIELD KEY (MapID::TransitionOutput(TransitionOutputMap::Public), "TransitionOutput(Public)"),
FIELD KEY (MapID::TransitionOutput(TransitionOutputMap::Private), "TransitionOutput(Private)"),
FIELD KEY (MapID::TransitionOutput(TransitionOutputMap::Record), "TransitionOutput(Record)"),
GROUP -> FIELD (nonce to commitment) (MapID::TransitionOutput(TransitionOutputMap::RecordNonce), "TransitionOutput(RecordNonce)"),
FIELD ENTRY?? (MapID::TransitionOutput(TransitionOutputMap::ExternalRecord), "TransitionOutput(ExternalRecord)"),
FIELD KEY (MapID::TransitionOutput(TransitionOutputMap::Future), "TransitionOutput(Future)"),

// Transaction
TRANSACTION ID KEY -> TX TYPE
(MapID::Transaction(TransactionMap::ID), "Transaction(ID)"),

// Transition
ALL INSERTED BY TRANSITION ID
TRANSITION ID KEY (MapID::Transition(TransitionMap::Locator), "Transition(Locator)"),
TRANSITION ID KEY -> GROUP (MapID::Transition(TransitionMap::TPK), "Transition(TPK)"),
GROUP -> TRANSITION ID (MapID::Transition(TransitionMap::ReverseTPK), "Transition(ReverseTPK)"),
TRANSITION ID KEY -> FIELD (MapID::Transition(TransitionMap::TCM), "Transition(TCM)"),
FIELD -> TRANSITION ID (MapID::Transition(TransitionMap::ReverseTCM), "Transition(ReverseTCM)"),
TRANSITION ID KEY (MapID::Transition(TransitionMap::SCM), "Transition(SCM)"),

// Program
INSERTED NO BLOCK ID
PROGRAM ID KEY -> SET OF IDENTIFIERS (MapID::Program(ProgramMap::ProgramID), "Program(ProgramID)"),
PROGRAM ID + IDENTIFIER -> MUTATED (MapID::Program(ProgramMap::KeyValueID),"Program(KeyValueID)"),

*/
