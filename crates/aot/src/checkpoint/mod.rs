use std::{
    borrow::Cow,
    fmt::Display,
    path::{Path, PathBuf},
    sync::Arc,
};

use aleo_std::StorageMode;
use anyhow::{bail, ensure, Result};
use rayon::iter::ParallelIterator;
use snarkos_node::bft::{
    helpers::Storage, ledger_service::CoreLedgerService, storage_service::BFTMemoryService,
};
use snarkvm::{
    console::program::Network as NetworkTrait,
    ledger::{
        authority::Authority,
        store::{
            helpers::{rocksdb::*, Map, MapRead},
            *,
        },
        Ledger,
    },
};

mod content;
mod header;
mod manager;
mod retention;

#[cfg(test)]
mod retention_tests;

pub use content::*;
pub use header::*;
pub use manager::*;
pub use retention::*;

pub fn path_from_storage<D: Display>(mode: &StorageMode, height: D) -> Option<PathBuf> {
    match mode {
        StorageMode::Custom(path) => path_from_height(path, height),
        _ => None,
    }
}

pub fn path_from_height<D: Display>(path: &Path, height: D) -> Option<PathBuf> {
    path.parent()
        .map(|p| p.join(format!("{height}.checkpoint")))
}

struct Stores<N: NetworkTrait> {
    blocks: BlockDB<N>,
    committee: CommitteeDB<N>,
    transactions: TransactionDB<N>,
    deployments: DeploymentDB<N>,
    executions: ExecutionDB<N>,
    fees: FeeDB<N>,
    transition: TransitionDB<N>,
    finalize: FinalizeDB<N>,
    inputs: InputDB<N>,
    outputs: OutputDB<N>,
}

impl<N: NetworkTrait> Stores<N> {
    fn open(storage_mode: StorageMode) -> Result<Self> {
        let transition_store = TransitionStore::<N, TransitionDB<N>>::open(storage_mode.clone())?;
        let fee_store = FeeStore::<N, FeeDB<N>>::open(transition_store.clone())?;
        Ok(Self {
            blocks: BlockDB::<N>::open(storage_mode.clone())?,
            committee: CommitteeDB::<N>::open(storage_mode.clone())?,
            transactions: TransactionDB::<N>::open(transition_store.clone())?,
            transition: TransitionDB::<N>::open(storage_mode.clone())?,
            deployments: DeploymentDB::<N>::open(fee_store.clone())?,
            executions: ExecutionDB::<N>::open(fee_store.clone())?,
            fees: FeeDB::<N>::open(transition_store)?,
            finalize: FinalizeDB::<N>::open(storage_mode.clone())?,
            inputs: InputDB::<N>::open(storage_mode.clone())?,
            outputs: OutputDB::<N>::open(storage_mode)?,
        })
    }
}

impl<N: NetworkTrait> Checkpoint<N> {
    pub fn new_from_header(path: PathBuf, header: CheckpointHeader<N>) -> Result<Self> {
        let content = CheckpointContent::read_ledger(path)?;
        Ok(Self { header, content })
    }

    pub fn new(path: PathBuf) -> Result<Self> {
        let header = CheckpointHeader::read_ledger(path.clone())?;
        let content = CheckpointContent::read_ledger(path)?;

        Ok(Self { header, content })
    }

    pub fn check(&self, storage_mode: StorageMode) -> Result<()> {
        let blocks = BlockDB::<N>::open(storage_mode.clone())?;
        let committee = CommitteeDB::<N>::open(storage_mode.clone())?;
        let height = committee.current_height()?;

        ensure!(
            height > self.height(),
            "checkpoint is for a height greater than the current height"
        );
        ensure!(
            blocks.get_block_hash(self.height())? == Some(self.header.block_hash),
            "checkpoint block hash does not appear to belong to the block at the checkpoint height"
        );

        Ok(())
    }

    pub fn rewind(
        self,
        ledger: &Ledger<N, ConsensusDB<N>>,
        storage_mode: StorageMode,
    ) -> Result<()> {
        use rayon::iter::IntoParallelIterator;

        let stores = Stores::<N>::open(storage_mode.clone())?;

        self.check(storage_mode)?;

        let height = stores.committee.current_height()?;
        let my_height = self.height();

        // the act of creating this ledger service with a "max_gc_rounds" set to 0 should clear
        // all BFT documents
        let ledger_service = Arc::new(CoreLedgerService::new(ledger.clone(), Default::default()));
        Storage::new(ledger_service, Arc::new(BFTMemoryService::new()), 0);

        // TODO: parallel and test out of order removal by moving the guts of these functions out of the "atomic writes"
        ((my_height + 1)..=height)
            .into_par_iter()
            .try_for_each(|h| {
                fast_block_remove(&stores, h)
                    .and_then(|_| fast_committee_remove(&stores.committee, h))
            })?;

        // TODO: diff the programs so we don't have to remove the mappings

        // delete old mappings (can make this parallel)
        for (prog, mappings) in stores.finalize.program_id_map().iter_confirmed() {
            for mapping in mappings.iter() {
                stores.finalize.remove_mapping(*prog, *mapping)?;
            }
        }

        // write replacement mappings
        for ((prog, mapping), entries) in self.content.key_values.into_iter() {
            stores.finalize.initialize_mapping(prog, mapping)?;
            stores.finalize.replace_mapping(prog, mapping, entries)?;
        }

        // set the current round to the last round in the new top block
        // using the committee store to determine what the first round of the new top block is
        if let Some(c) = stores.committee.get_committee(my_height)? {
            let mut round = c.starting_round();
            // loop until the the next round is different (it will be None, but this is cleaner)
            while stores.committee.get_height_for_round(round + 1)? == Some(height) {
                round += 1;
            }
            stores
                .committee
                .current_round_map()
                .insert(ROUND_KEY, round)?;
        } else {
            bail!("no committee found for height {my_height}. ledger is likely corrupted",);
        }

        Ok(())
    }

    pub fn height(&self) -> u32 {
        self.header.block_height
    }

    pub fn header(&self) -> &CheckpointHeader<N> {
        &self.header
    }
}

// The following functions are effectively gutted versions of the snarkvm ledger removal functions
// that do not have any atomic locks

fn fast_block_remove<N: NetworkTrait>(stores: &Stores<N>, block_height: u32) -> Result<()> {
    let db = &stores.blocks;

    let Some(block_hash) = db.get_block_hash(block_height)? else {
        bail!("failed to remove block: missing block hash for height '{block_height}'");
    };

    // Retrieve the state root.
    let state_root = match db.state_root_map().get_confirmed(&block_height)? {
        Some(state_root) => cow_to_copied!(state_root),
        None => {
            bail!("Failed to remove block: missing state root for block height '{block_height}'")
        }
    };
    // Retrieve the transaction IDs.
    let transaction_ids = match db.transactions_map().get_confirmed(&block_hash)? {
            Some(transaction_ids) => transaction_ids,
            None => bail!("Failed to remove block: missing transactions for block '{block_height}' ('{block_hash}')"),
        };
    // Retrieve the solutions.
    let solutions = match db.solutions_map().get_confirmed(&block_hash)? {
        Some(solutions) => cow_to_cloned!(solutions),
        None => {
            bail!("Failed to remove block: missing solutions for block '{block_height}' ('{block_hash}')")
        }
    };

    // Retrieve the aborted solution IDs.
    let aborted_solution_ids = match db.get_block_aborted_solution_ids(&block_hash)? {
        Some(solution_ids) => solution_ids,
        None => Vec::new(),
    };

    // Retrieve the aborted transaction IDs.
    let aborted_transaction_ids = match db.get_block_aborted_transaction_ids(&block_hash)? {
        Some(transaction_ids) => transaction_ids,
        None => Vec::new(),
    };

    // Retrieve the rejected transaction IDs, and the deployment or execution ID.
    let rejected_transaction_ids_and_deployment_or_execution_id =
        match db.get_block_transactions(&block_hash)? {
            Some(transactions) => transactions
                .iter()
                .filter(|tx| tx.is_rejected())
                .map(|tx| Ok((tx.to_unconfirmed_transaction_id()?, tx.to_rejected_id()?)))
                .collect::<Result<Vec<_>>>()?,
            None => Vec::new(),
        };

    // Determine the certificate IDs to remove.
    let certificate_ids_to_remove = match db.authority_map().get_confirmed(&block_hash)? {
        Some(authority) => match authority {
            Cow::Owned(Authority::Beacon(_)) | Cow::Borrowed(Authority::Beacon(_)) => Vec::new(),
            Cow::Owned(Authority::Quorum(ref subdag))
            | Cow::Borrowed(Authority::Quorum(ref subdag)) => {
                subdag.values().flatten().map(|c| c.id()).collect()
            }
        },
        None => bail!(
            "Failed to remove block: missing authority for block '{block_height}' ('{block_hash}')"
        ),
    };

    // Remove the (block height, state root) pair.
    db.state_root_map().remove(&block_height)?;
    // Remove the (state root, block height) pair.
    db.reverse_state_root_map().remove(&state_root)?;

    // Remove the block hash.
    db.id_map().remove(&block_height)?;
    // Remove the block height.
    db.reverse_id_map().remove(&block_hash)?;
    // Remove the block header.
    db.header_map().remove(&block_hash)?;

    // Remove the block authority.
    db.authority_map().remove(&block_hash)?;

    // Remove the block certificates.
    for certificate_id in certificate_ids_to_remove.iter() {
        db.certificate_map().remove(certificate_id)?;
    }

    // Remove the block ratifications.
    db.ratifications_map().remove(&block_hash)?;

    // Remove the block solutions.
    db.solutions_map().remove(&block_hash)?;

    // Remove the block solution IDs.
    for solution_id in solutions.solution_ids() {
        db.puzzle_commitments_map().remove(solution_id)?;
    }

    // Remove the aborted solution IDs.
    db.aborted_solution_ids_map().remove(&block_hash)?;

    // Remove the aborted solution heights.
    for solution_id in aborted_solution_ids {
        db.aborted_solution_heights_map().remove(&solution_id)?;
    }

    // Remove the transaction IDs.
    db.transactions_map().remove(&block_hash)?;

    // Remove the aborted transaction IDs.
    db.aborted_transaction_ids_map().remove(&block_hash)?;
    for aborted_transaction_id in aborted_transaction_ids {
        db.rejected_or_aborted_transaction_id_map()
            .remove(&aborted_transaction_id)?;
    }

    // Remove the rejected state.
    for (rejected_transaction_id, rejected_id) in
        rejected_transaction_ids_and_deployment_or_execution_id
    {
        // Remove the rejected transaction ID.
        db.rejected_or_aborted_transaction_id_map()
            .remove(&rejected_transaction_id)?;
        // Remove the rejected deployment or execution.
        if let Some(rejected_id) = rejected_id {
            db.rejected_deployment_or_execution_map()
                .remove(&rejected_id)?;
        }
    }

    // Remove the block transactions.
    for transaction_id in transaction_ids.iter() {
        // Remove the reverse transaction ID.
        db.confirmed_transactions_map().remove(transaction_id)?;
        // Remove the transaction.
        fast_tx_remove(stores, transaction_id)?;
    }

    Ok(())
}

fn fast_tx_remove<N: NetworkTrait>(
    stores: &Stores<N>,
    transaction_id: &N::TransactionID,
) -> Result<()> {
    let db = &stores.transactions;

    // Retrieve the transaction type.
    let transaction_type = match db.id_map().get_confirmed(transaction_id)? {
        Some(transaction_type) => cow_to_copied!(transaction_type),
        None => bail!("Failed to get the type for transaction '{transaction_id}'"),
    };

    // Remove the transaction type.
    db.id_map().remove(transaction_id)?;
    // Remove the transaction.
    match transaction_type {
        // Remove the deployment transaction.
        TransactionType::Deploy => fast_deployment_remove(stores, transaction_id)?,
        // Remove the execution transaction.
        TransactionType::Execute => fast_execution_remove(stores, transaction_id)?,
        // Remove the fee transaction.
        TransactionType::Fee => fast_fee_remove(stores, transaction_id)?,
    }
    Ok(())
}

fn fast_deployment_remove<N: NetworkTrait>(
    stores: &Stores<N>,
    transaction_id: &N::TransactionID,
) -> Result<()> {
    let db = &stores.deployments;

    // Retrieve the program ID.
    let program_id = match db.get_program_id(transaction_id)? {
        Some(edition) => edition,
        None => bail!("Failed to get the program ID for transaction '{transaction_id}'"),
    };
    // Retrieve the edition.
    let edition = match db.get_edition(&program_id)? {
        Some(edition) => edition,
        None => bail!("Failed to locate the edition for program '{program_id}'"),
    };
    // Retrieve the program.
    let program = match db.program_map().get_confirmed(&(program_id, edition))? {
        Some(program) => cow_to_cloned!(program),
        None => bail!("Failed to locate program '{program_id}' for transaction '{transaction_id}'"),
    };

    // Remove the program ID.
    db.id_map().remove(transaction_id)?;
    // Remove the edition.
    db.edition_map().remove(&program_id)?;

    // Remove the reverse program ID.
    db.reverse_id_map().remove(&(program_id, edition))?;
    // Remove the owner.
    db.owner_map().remove(&(program_id, edition))?;
    // Remove the program.
    db.program_map().remove(&(program_id, edition))?;

    // Remove the verifying keys and certificates.
    for function_name in program.functions().keys() {
        // Remove the verifying key.
        db.verifying_key_map()
            .remove(&(program_id, *function_name, edition))?;
        // Remove the certificate.
        db.certificate_map()
            .remove(&(program_id, *function_name, edition))?;
    }

    // Remove the fee transition.
    fast_fee_remove(stores, transaction_id)?;

    Ok(())
}

fn fast_execution_remove<N: NetworkTrait>(
    stores: &Stores<N>,
    transaction_id: &N::TransactionID,
) -> Result<()> {
    let db = &stores.executions;

    // Retrieve the transition IDs and fee boolean.
    let (transition_ids, has_fee) = match db.id_map().get_confirmed(transaction_id)? {
        Some(ids) => cow_to_cloned!(ids),
        None => bail!("Failed to get the transition IDs for the transaction '{transaction_id}'"),
    };

    // Remove the transition IDs.
    db.id_map().remove(transaction_id)?;

    // Remove the execution.
    for transition_id in transition_ids {
        // Remove the transition ID.
        db.reverse_id_map().remove(&transition_id)?;
        // Remove the transition.
        fast_transition_remove(stores, &transition_id)?;
    }

    // Remove the global state root and proof.
    db.inclusion_map().remove(transaction_id)?;

    // Remove the fee.
    if has_fee {
        // Remove the fee.
        fast_fee_remove(stores, transaction_id)?;
    }

    Ok(())
}

fn fast_fee_remove<N: NetworkTrait>(
    stores: &Stores<N>,
    transaction_id: &N::TransactionID,
) -> Result<()> {
    let db = &stores.fees;

    // Retrieve the fee transition ID.
    let (transition_id, _, _) = match db.fee_map().get_confirmed(transaction_id)? {
        Some(fee_id) => cow_to_cloned!(fee_id),
        None => bail!("Failed to locate the fee transition ID for transaction '{transaction_id}'"),
    };

    // Remove the fee.
    db.fee_map().remove(transaction_id)?;
    db.reverse_fee_map().remove(&transition_id)?;

    // Remove the fee transition.
    fast_transition_remove(stores, &transition_id)?;

    Ok(())
}

fn fast_transition_remove<N: NetworkTrait>(
    stores: &Stores<N>,
    transition_id: &N::TransitionID,
) -> Result<()> {
    let db = &stores.transition;

    // Retrieve the `tpk`.
    let tpk = match db.tpk_map().get_confirmed(transition_id)? {
        Some(tpk) => cow_to_copied!(tpk),
        None => return Ok(()),
    };
    // Retrieve the `tcm`.
    let tcm = match db.tcm_map().get_confirmed(transition_id)? {
        Some(tcm) => cow_to_copied!(tcm),
        None => return Ok(()),
    };

    // Remove the program ID and function name.
    db.locator_map().remove(transition_id)?;
    // Remove the inputs.
    fast_input_remove(stores, transition_id)?;
    // Remove the outputs.
    fast_output_remove(stores, transition_id)?;
    // Remove `tpk`.
    db.tpk_map().remove(transition_id)?;
    // Remove the reverse `tpk` entry.
    db.reverse_tpk_map().remove(&tpk)?;
    // Remove `tcm`.
    db.tcm_map().remove(transition_id)?;
    // Remove the reverse `tcm` entry.
    db.reverse_tcm_map().remove(&tcm)?;
    // Remove `scm`.
    db.scm_map().remove(transition_id)?;

    Ok(())
}

fn fast_committee_remove<N: NetworkTrait>(db: &CommitteeDB<N>, height: u32) -> Result<()> {
    // Retrieve the committee for the given height.
    let Some(committee) = db.get_committee(height)? else {
        bail!("Committee not found for height {height} in committee storage");
    };
    // Retrieve the round for the given height.
    let committee_round = committee.starting_round();

    // Find the earliest round to be removed (inclusive).
    let mut earliest_round = committee_round;
    while earliest_round > 0 && db.get_height_for_round(earliest_round)? == Some(height) {
        earliest_round = earliest_round.saturating_sub(1);
    }
    let is_multiple = earliest_round != committee_round;
    if is_multiple {
        earliest_round += 1;
    }

    // Find the latest round to be removed (exclusive).
    let mut latest_round = committee_round;
    while db.get_height_for_round(latest_round)? == Some(height) {
        latest_round = latest_round.saturating_add(1);
    }

    // Remove the round to height mappings.
    for round in earliest_round..latest_round {
        db.round_to_height_map().remove(&round)?;
    }
    // Remove the committee.
    db.committee_map().remove(&height)?;

    Ok(())
}

fn fast_input_remove<N: NetworkTrait>(
    stores: &Stores<N>,
    transition_id: &N::TransitionID,
) -> Result<()> {
    let db = &stores.inputs;

    // Retrieve the input IDs.
    let input_ids: Vec<_> = match db.id_map().get_confirmed(transition_id)? {
        Some(Cow::Borrowed(ids)) => ids.to_vec(),
        Some(Cow::Owned(ids)) => ids.into_iter().collect(),
        None => return Ok(()),
    };

    // Remove the input IDs.
    db.id_map().remove(transition_id)?;

    // Remove the inputs.
    for input_id in input_ids {
        // Remove the reverse input ID.
        db.reverse_id_map().remove(&input_id)?;

        // If the input is a record, remove the record tag.
        if let Some(tag) = db.record_map().get_confirmed(&input_id)? {
            db.record_tag_map().remove(&tag)?;
        }

        // Remove the input.
        db.constant_map().remove(&input_id)?;
        db.public_map().remove(&input_id)?;
        db.private_map().remove(&input_id)?;
        db.record_map().remove(&input_id)?;
        db.external_record_map().remove(&input_id)?;
    }

    Ok(())
}

fn fast_output_remove<N: NetworkTrait>(
    stores: &Stores<N>,
    transition_id: &N::TransitionID,
) -> Result<()> {
    let db = &stores.outputs;

    // Retrieve the output IDs.
    let output_ids: Vec<_> = match db.id_map().get_confirmed(transition_id)? {
        Some(Cow::Borrowed(ids)) => ids.to_vec(),
        Some(Cow::Owned(ids)) => ids.into_iter().collect(),
        None => return Ok(()),
    };

    // Remove the output IDs.
    db.id_map().remove(transition_id)?;

    // Remove the outputs.
    for output_id in output_ids {
        // Remove the reverse output ID.
        db.reverse_id_map().remove(&output_id)?;

        // If the output is a record, remove the record nonce.
        if let Some(record) = db.record_map().get_confirmed(&output_id)? {
            if let Some(record) = &record.1 {
                db.record_nonce_map().remove(record.nonce())?;
            }
        }

        // Remove the output.
        db.constant_map().remove(&output_id)?;
        db.public_map().remove(&output_id)?;
        db.private_map().remove(&output_id)?;
        db.record_map().remove(&output_id)?;
        db.external_record_map().remove(&output_id)?;
        db.future_map().remove(&output_id)?;
    }

    Ok(())
}
