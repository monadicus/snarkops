use anyhow::{bail, Ok, Result};
use clap::Args;
use clap_stdin::MaybeStdin;
use rand::{CryptoRng, Rng};
use snarkvm::{
    ledger::Deployment,
    prelude::Field,
    synthesizer::{
        process::{cost_in_microcredits, deployment_cost},
        Process,
    },
    utilities::ToBytes,
};

use super::query;
// use tracing::error;
use crate::{Authorization, Key, Network, PTRecord, PrivateKey};

/// The authorization arguments for a fee.
#[derive(Debug, Args)]
pub struct AuthFeeOptions<N: Network> {
    /// The priority fee in microcredits.
    #[clap(long, default_value_t = 0)]
    pub priority_fee: u64,
    /// The record for a private fee.
    #[clap(long)]
    pub record: Option<PTRecord<N>>,
}

/// Authorize the fee for a program execution.
#[derive(Debug, Args)]
pub struct AuthorizeFee<N: Network> {
    #[clap(flatten)]
    pub key: Key<N>,
    #[clap(flatten)]
    pub options: AuthFeeOptions<N>,
    /// The query to use for the program execution cost lookup
    #[clap(long, group = "program")]
    pub query: Option<String>,
    /// The Authorization for the program execution
    #[arg(short, long, group = "program")]
    pub auth: Option<MaybeStdin<Authorization<N>>>,
    /// The Authorization for a deployment
    #[arg(short, long, group = "deploy")]
    pub deployment: Option<MaybeStdin<Deployment<N>>>,
    /// The ID of the deployment or program execution
    #[arg(short, long, group = "manual")]
    pub id: Option<Field<N>>,
    /// Estimated cost of the deployment or program execution
    #[arg(short, long, group = "manual")]
    pub cost: Option<u64>,
    /// The seed to use for the authorization generation
    #[clap(long)]
    pub seed: Option<u64>,
}

impl<N: Network> AuthorizeFee<N> {
    pub fn parse(self) -> Result<Option<Authorization<N>>> {
        let (id, base_fee) = match (self.auth, self.deployment, self.id, self.cost) {
            (Some(auth), None, None, None) => {
                let auth = auth.into_inner();
                let mut process = Process::load_no_storage()?;
                if let Some(query) = self.query.as_deref() {
                    let programs = query::get_programs_from_auth(&auth);
                    query::add_many_programs_to_process(&mut process, programs, query)?;
                }

                (auth.to_execution_id()?, estimate_cost(&process, &auth)?)
            }
            (None, Some(deployment), None, None) => {
                let deployment = deployment.into_inner();
                (
                    deployment.to_deployment_id()?,
                    deployment_cost(&deployment)?.0,
                )
            }
            (None, None, Some(id), Some(cost)) => (id, cost),
            _ => bail!("Exactly one of auth, deployment, or id and cost must be provided"),
        };

        let fee = fee_auth(
            id,
            base_fee,
            self.key.try_get()?,
            self.options.priority_fee,
            &mut super::rng_from_seed(self.seed),
            self.options.record,
        )?;

        Ok(fee)
    }
}

pub fn fee_auth<N: Network>(
    execution_id: Field<N>,
    base_fee_in_microcredits: u64,
    private_key: PrivateKey<N>,
    priority_fee_in_microcredits: u64,
    rng: &mut (impl Rng + CryptoRng),
    record: Option<PTRecord<N>>,
) -> Result<Option<Authorization<N>>> {
    if base_fee_in_microcredits == 0 && priority_fee_in_microcredits == 0 {
        return Ok(None);
    }

    let process = N::process();

    // Authorize the fee.
    let fee = if let Some(record) = record {
        process.authorize_fee_private::<N::Circuit, _>(
            &private_key,
            record,
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

pub fn estimate_cost<N: Network>(process: &Process<N>, func: &Authorization<N>) -> Result<u64> {
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

        // storage cost multipliers.... snarkvm#2456
        if cost > N::EXECUTION_STORAGE_PENALTY_THRESHOLD {
            cost = cost
                .saturating_mul(cost)
                .saturating_div(N::EXECUTION_STORAGE_FEE_SCALING_FACTOR);
        }

        cost
    };
    //execution.size_in_bytes().map_err(|e| e.to_string())?;

    // Compute the finalize cost in microcredits.
    let mut finalize_cost = 0u64;
    // Iterate over the transitions to accumulate the finalize cost.
    for (_key, transition) in transitions {
        // Retrieve the function name, program id, and program.
        let function_name = *transition.function_name();
        let stack = process.get_stack(transition.program_id())?;
        let cost = cost_in_microcredits(&stack, &function_name)?;

        // Accumulate the finalize cost.
        if let Some(cost) = finalize_cost.checked_add(cost) {
            finalize_cost = cost;
        } else {
            bail!("The finalize cost computation overflowed for an execution")
        };
    }
    Ok(storage_cost + finalize_cost)
}
