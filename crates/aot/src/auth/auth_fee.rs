use anyhow::{bail, Ok, Result};
use clap::Args;
use rand::{CryptoRng, Rng};
use snarkvm::{
    ledger::Deployment,
    prelude::Field,
    synthesizer::process::{cost_in_microcredits, deployment_cost},
    utilities::ToBytes,
};

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
    /// The Authorization for the program program execution
    #[arg(short, long, group = "program")]
    pub auth: Option<Authorization<N>>,
    #[arg(short, long, group = "deployment")]
    pub deployment: Option<Deployment<N>>,
    /// The ID of the deployment or program execution
    #[arg(short, long, group = "manual")]
    pub id: Option<Field<N>>,
    /// Estimated cost of the deployment or program execution
    #[arg(short, long, group = "manual")]
    pub cost: Option<u64>,
}

impl<N: Network> AuthorizeFee<N> {
    pub fn parse(self) -> Result<Option<Authorization<N>>> {
        let (id, base_fee) = match (self.auth, self.deployment, self.id, self.cost) {
            (Some(auth), None, None, None) => (auth.to_execution_id()?, estimate_cost(&auth)?),
            (None, Some(deployment), None, None) => (
                deployment.to_deployment_id()?,
                deployment_cost(&deployment)?.0,
            ),
            (None, None, Some(id), Some(cost)) => (id, cost),
            _ => bail!("Exactly one of auth, deployment, or id and cost must be provided"),
        };

        let fee = fee_auth(
            id,
            base_fee,
            self.key.try_get()?,
            self.options.priority_fee,
            &mut rand::thread_rng(),
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
