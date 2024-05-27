use std::borrow::Cow;

use anyhow::{bail, Result};
use snarkvm::console::program::Network;

use crate::aleo::*;

pub struct Stores<N: Network> {
    pub blocks: BlockDB<N>,
    pub committee: CommitteeDB<N>,
    pub transactions: TransactionDB<N>,
    pub deployments: DeploymentDB<N>,
    pub executions: ExecutionDB<N>,
    pub fees: FeeDB<N>,
    pub transition: TransitionDB<N>,
    pub finalize: FinalizeDB<N>,
    pub inputs: InputDB<N>,
    pub outputs: OutputDB<N>,
}

impl<N: Network> Stores<N> {
    pub fn open(storage_mode: StorageMode) -> Result<Self> {
        let transition_store = TransitionStore::open(storage_mode.clone())?;
        let fee_store = FeeStore::open(transition_store.clone())?;
        Ok(Self {
            blocks: BlockDB::open(storage_mode.clone())?,
            committee: CommitteeDB::open(storage_mode.clone())?,
            transactions: TransactionDB::open(transition_store.clone())?,
            transition: TransitionDB::open(storage_mode.clone())?,
            deployments: DeploymentDB::open(fee_store.clone())?,
            executions: ExecutionDB::open(fee_store.clone())?,
            fees: FeeDB::open(transition_store)?,
            finalize: FinalizeDB::open(storage_mode.clone())?,
            inputs: InputDB::open(storage_mode.clone())?,
            outputs: OutputDB::open(storage_mode)?,
        })
    }

    pub fn remove(&self, height: u32) -> Result<()> {
        self.fast_block_remove(height)?;
        self.fast_committee_remove(height)
    }

    // The following functions are effectively gutted versions of the snarkvm ledger
    // removal functions that do not have any atomic locks

    fn fast_block_remove(&self, block_height: u32) -> Result<()> {
        let db = &self.blocks;

        let Some(block_hash) = db.get_block_hash(block_height)? else {
            bail!("failed to remove block: missing block hash for height '{block_height}'");
        };

        // Retrieve the state root.
        let state_root = match db.state_root_map().get_confirmed(&block_height)? {
            Some(state_root) => cow_to_copied!(state_root),
            None => {
                bail!(
                    "Failed to remove block: missing state root for block height '{block_height}'"
                )
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
        let aborted_solution_ids =
            (db.get_block_aborted_solution_ids(&block_hash)?).unwrap_or_default();

        // Retrieve the aborted transaction IDs.
        let aborted_transaction_ids =
            (db.get_block_aborted_transaction_ids(&block_hash)?).unwrap_or_default();

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
                Cow::Owned(Authority::Beacon(_)) | Cow::Borrowed(Authority::Beacon(_)) => {
                    Vec::new()
                }
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
            db.solution_ids_map().remove(solution_id)?;
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
            self.fast_tx_remove(transaction_id)?;
        }

        Ok(())
    }

    fn fast_tx_remove(&self, transaction_id: &TransactionID<N>) -> Result<()> {
        let db = &self.transactions;

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
            TransactionType::Deploy => self.fast_deployment_remove(transaction_id)?,
            // Remove the execution transaction.
            TransactionType::Execute => self.fast_execution_remove(transaction_id)?,
            // Remove the fee transaction.
            TransactionType::Fee => self.fast_fee_remove(transaction_id)?,
        }
        Ok(())
    }

    fn fast_deployment_remove(&self, transaction_id: &TransactionID<N>) -> Result<()> {
        let db = &self.deployments;

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
            None => {
                bail!("Failed to locate program '{program_id}' for transaction '{transaction_id}'")
            }
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
        self.fast_fee_remove(transaction_id)?;

        Ok(())
    }

    fn fast_execution_remove(&self, transaction_id: &TransactionID<N>) -> Result<()> {
        let db = &self.executions;

        // Retrieve the transition IDs and fee boolean.
        let (transition_ids, has_fee) = match db.id_map().get_confirmed(transaction_id)? {
            Some(ids) => cow_to_cloned!(ids),
            None => {
                bail!("Failed to get the transition IDs for the transaction '{transaction_id}'")
            }
        };

        // Remove the transition IDs.
        db.id_map().remove(transaction_id)?;

        // Remove the execution.
        for transition_id in transition_ids {
            // Remove the transition ID.
            db.reverse_id_map().remove(&transition_id)?;
            // Remove the transition.
            self.fast_transition_remove(&transition_id)?;
        }

        // Remove the global state root and proof.
        db.inclusion_map().remove(transaction_id)?;

        // Remove the fee.
        if has_fee {
            // Remove the fee.
            self.fast_fee_remove(transaction_id)?;
        }

        Ok(())
    }

    fn fast_fee_remove(&self, transaction_id: &TransactionID<N>) -> Result<()> {
        let db = &self.fees;

        // Retrieve the fee transition ID.
        let (transition_id, _, _) = match db.fee_map().get_confirmed(transaction_id)? {
            Some(fee_id) => cow_to_cloned!(fee_id),
            None => {
                bail!("Failed to locate the fee transition ID for transaction '{transaction_id}'")
            }
        };

        // Remove the fee.
        db.fee_map().remove(transaction_id)?;
        db.reverse_fee_map().remove(&transition_id)?;

        // Remove the fee transition.
        self.fast_transition_remove(&transition_id)?;

        Ok(())
    }

    fn fast_transition_remove(&self, transition_id: &TransitionID<N>) -> Result<()> {
        let db = &self.transition;

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
        self.fast_input_remove(transition_id)?;
        // Remove the outputs.
        self.fast_output_remove(transition_id)?;
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

    fn fast_committee_remove(&self, height: u32) -> Result<()> {
        let db = &self.committee;
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

    fn fast_input_remove(&self, transition_id: &TransitionID<N>) -> Result<()> {
        let db = &self.inputs;

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

    fn fast_output_remove(&self, transition_id: &TransitionID<N>) -> Result<()> {
        let db = &self.outputs;

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
}
