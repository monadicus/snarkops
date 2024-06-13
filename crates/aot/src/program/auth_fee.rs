use anyhow::{bail, Ok, Result};
use clap::Args;
use rand::{CryptoRng, Rng};
use snarkvm::{synthesizer::process::cost_in_microcredits, utilities::ToBytes};

// use tracing::error;
use crate::{runner::Key, Authorization, Network, PTRecord, PrivateKey};

#[derive(Debug, Args)]
pub struct AuthFeeOptions<N: Network> {
    /// The priority fee in microcredits.
    #[clap(long, default_value_t = 0)]
    pub priority_fee: u64,
    /// The record for a private fee.
    #[clap(long)]
    pub record: Option<PTRecord<N>>,
}

#[derive(Debug, Args)]
pub struct AuthorizeFee<N: Network> {
    #[clap(flatten)]
    pub key: Key<N>,
    #[clap(flatten)]
    pub options: AuthFeeOptions<N>,
    /// The Authorization for the function.
    #[arg(short, long)]
    pub auth: Authorization<N>,
}

impl<N: Network> AuthorizeFee<N> {
    pub fn parse(self) -> Result<Option<Authorization<N>>> {
        let fee = fee(
            self.auth,
            self.key.try_get()?,
            self.options.priority_fee,
            &mut rand::thread_rng(),
            self.options.record,
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

    if base_fee_in_microcredits == 0 && priority_fee_in_microcredits == 0 {
        return Ok(None);
    }

    let process = N::process();

    // Authorize the fee.
    let fee = if record.is_some() {
        process.authorize_fee_private::<N::Circuit, _>(
            &private_key,
            record.unwrap(),
            base_fee_in_microcredits,
            priority_fee_in_microcredits,
            execution_id,
            rng,
        )?
    } else {
        process.authorize_fee_public::<N::Circuit, _>(
            &private_key,
            base_fee_in_microcredits,
            priority_fee_in_microcredits,
            execution_id,
            rng,
        )?
    };

    Ok(Some(fee))
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
        false => Some(N::process().authorize_fee_private::<N::Circuit, _>(
            &private_key,
            credits,
            base_fee_in_microcredits,
            priority_fee_in_microcredits,
            execution_id,
            rng,
        )?),
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
        let function_name = *transition.function_name();
        let stack = N::process().get_stack(transition.program_id().to_string())?;
        let cost = cost_in_microcredits(stack, &function_name)?;

        // Accumulate the finalize cost.
        if let Some(cost) = finalize_cost.checked_add(cost) {
            finalize_cost = cost;
        } else {
            bail!("The finalize cost computation overflowed for an execution")
        };
    }
    Ok(storage_cost + finalize_cost)
}
