use anyhow::{bail, Ok, Result};
use clap::Args;
use rand::{CryptoRng, Rng};
use snarkvm::{
    console::types::Field,
    ledger::Transition,
    synthesizer::{cast_ref, process::cost_in_microcredits},
    utilities::ToBytes,
};

// use tracing::error;
use crate::{mux_process, Authorization, Network, PTRecord, PrivateKey};

#[derive(Debug, Args)]
pub struct AuthorizeFee<N: Network> {
    #[arg(short, long)]
    pub private_key: PrivateKey<N>,
    /// The Authorization for the function.
    #[arg(short, long)]
    pub authorization: Authorization<N>,
    /// The priority fee in microcredits.
    #[clap(long, default_value_t = 0)]
    pub priority_fee: u64,
    /// The record for a private fee.
    #[clap(long)]
    pub record: Option<PTRecord<N>>,
}

impl<N: Network> AuthorizeFee<N> {
    pub fn parse(self) -> Result<Option<Authorization<N>>> {
        let fee = fee(
            self.authorization,
            self.private_key,
            self.priority_fee,
            &mut rand::thread_rng(),
            self.record,
        )?;

        Ok(fee)
    }
}

pub fn fee<N: Network>(
    auth: Authorization<N>,
    private_key: PrivateKey<N>,
    priority_fee_in_microcredits: u64,
    rng: &mut (impl Rng + CryptoRng),
    record: Option<PTRecord<N>>,
) -> Result<Option<Authorization<N>>> {
    // Retrieve the execution ID.
    let execution_id: snarkvm::prelude::Field<N> = auth.to_execution_id()?;
    // Determine the base fee in microcredits.
    let base_fee_in_microcredits = estimate_cost(&auth)?;

    // Authorize the fee.
    let fee = match base_fee_in_microcredits == 0 && priority_fee_in_microcredits == 0 {
        true => None,
        false if record.is_some() => Some(mux_process!(A, N, |process| {
            process.authorize_fee_private::<A, _>(
                cast_ref!(private_key as PrivateKey<N>),
                cast_ref!((record.unwrap()) as PTRecord<N>).clone(),
                base_fee_in_microcredits,
                priority_fee_in_microcredits,
                *cast_ref!(execution_id as Field<N>),
                rng,
            )?
        })),
        false => Some(mux_process!(A, N, |process| {
            process.authorize_fee_public::<A, _>(
                cast_ref!(private_key as PrivateKey<N>),
                base_fee_in_microcredits,
                priority_fee_in_microcredits,
                *cast_ref!(execution_id as Field<N>),
                rng,
            )?
        })),
    };

    Ok(fee)
}

pub fn fee_private<N: Network>(
    auth: Authorization<N>,
    private_key: PrivateKey<N>,
    credits: PTRecord<N>,
    priority_fee_in_microcredits: u64,
    rng: &mut (impl Rng + CryptoRng),
) -> Result<Option<Authorization<N>>> {
    // Retrieve the execution ID.
    let execution_id: snarkvm::prelude::Field<N> = auth.to_execution_id()?;
    // Determine the base fee in microcredits.
    let base_fee_in_microcredits = estimate_cost(&auth)?;

    // Authorize the fee.
    let fee = match base_fee_in_microcredits == 0 && priority_fee_in_microcredits == 0 {
        true => None,
        false => Some(mux_process!(A, N, |process| {
            process.authorize_fee_private::<A, _>(
                cast_ref!(private_key as PrivateKey<N>),
                cast_ref!(credits as PTRecord<N>).clone(),
                base_fee_in_microcredits,
                priority_fee_in_microcredits,
                *cast_ref!(execution_id as Field<N>),
                rng,
            )?
        })),
    };

    Ok(fee)
}

fn estimate_cost<N: Network>(func: &Authorization<N>) -> Result<u64> {
    let transitions = func.transitions();

    let storage_cost = {
        let mut cost = 0u64;

        cost += 1; // execution version, 1 byte
        cost += 1; // number of transitions, 1 byte

        // write each transition
        for transition in transitions.values() {
            cost += transition.to_bytes_le()?.len() as u64;
        }

        // state root (this is 32 bytes)
        cost += N::StateRoot::default().to_bytes_le()?.len() as u64;

        // proof option is_some (1 byte)
        cost += 1;
        // Proof<Network> version
        cost += 1;

        cost += 956; // size of proof with 1 batch size

        /* cost += varuna::Proof::<<Network as Environment>::PairingCurve>::new(
            todo!("batch_sizes"),
            todo!("commitments"),
            todo!("evaluations"),
            todo!("prover_third_message"),
            todo!("prover_fourth_message"),
            todo!("pc_proof"),
        )?
        .to_bytes_le()?
        .len() as u64; */

        cost
    };
    //execution.size_in_bytes().map_err(|e| e.to_string())?;

    // Compute the finalize cost in microcredits.
    let mut finalize_cost = 0u64;
    // Iterate over the transitions to accumulate the finalize cost.
    for (_key, transition) in transitions {
        // Retrieve the function name, program id, and program.
        let function_name = transition.function_name();
        let stack = mux_process!(_A, N, |process| {
            process.get_stack(cast_ref!(transition as Transition<N>).program_id())?
        });
        let cost = cost_in_microcredits(stack, function_name)?;

        // Accumulate the finalize cost.
        if let Some(cost) = finalize_cost.checked_add(cost) {
            finalize_cost = cost;
        } else {
            bail!("The finalize cost computation overflowed for an execution")
        };
    }
    Ok(storage_cost + finalize_cost)
}
