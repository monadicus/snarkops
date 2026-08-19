use anyhow::{Result, ensure};
use clap::Args;
use clap_stdin::FileOrStdin;
use snarkvm::{
    prelude::{ConsensusVersion, Identifier, Value},
    synthesizer::{Process, Program, process::deployment_cost},
};

use crate::{
    Network, PrivateKey,
    auth::{auth_fee::estimate_cost, query},
};

/// Compute the cost to execute a function in a given program.
#[derive(Debug, Args)]
pub struct CostCommand<N: Network> {
    /// Query to load the program with.
    #[clap(short, long)]
    pub query: Option<String>,
    /// Program to estimate the cost of.
    pub program: FileOrStdin<Program<N>>,
    /// Program ID and function name (eg. credits.aleo/transfer_public). When
    /// not specified, the cost of deploying the program is estimated.
    function: Option<Identifier<N>>,
    /// Program inputs (eg. 1u64 5field)
    #[clap(num_args = 1, value_delimiter = ' ')]
    inputs: Vec<Value<N>>,
    /// Enable dynamic block height for the transaction cost estimation (latest
    /// by default)
    #[clap(long)]
    pub height: Option<u32>,
}

pub fn consensus_from_height<N: Network>(height: Option<u32>) -> ConsensusVersion {
    if let Some(height) = height {
        N::CONSENSUS_VERSION(height).unwrap_or(ConsensusVersion::V1)
    } else {
        ConsensusVersion::latest()
    }
}

impl<N: Network> CostCommand<N> {
    pub fn parse(self) -> Result<u64> {
        let CostCommand {
            query,
            program,
            function,
            inputs,
            height,
        } = self;

        let program = program.contents()?;
        let process = Process::load()?;
        query::get_process_imports(&process, &program, query.as_deref())?;
        let v = consensus_from_height::<N>(height);

        if let Some(function) = function {
            process.lock().add_program(&program)?;
            ensure!(
                program.functions().contains_key(&function),
                "Function {} not found in program",
                function
            );

            let auth = process
                .get_stack(program.id())?
                .authorize::<N::Circuit, _>(
                    &PrivateKey::new(&mut rand::rng())?,
                    function,
                    inputs.iter(),
                    &mut rand::rng(),
                )?;

            estimate_cost(&process, &auth, v)
        } else {
            let deployment = process.deploy::<N::Circuit, _>(&program, &mut rand::rng())?;
            Ok(deployment_cost(&process, &deployment, v)?.0)
        }
    }
}
