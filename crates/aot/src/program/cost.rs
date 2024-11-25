use anyhow::{ensure, Result};
use clap::Args;
use clap_stdin::FileOrStdin;
use snarkvm::{
    prelude::{Identifier, Value},
    synthesizer::{process::deployment_cost, Process, Program},
};

use crate::{
    auth::{auth_fee::estimate_cost, query},
    Network, PrivateKey,
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
}

impl<N: Network> CostCommand<N> {
    pub fn parse(self) -> Result<u64> {
        let CostCommand {
            query,
            program,
            function,
            inputs,
        } = self;

        let program = program.contents()?;
        let mut process = Process::load()?;
        query::get_process_imports(&mut process, &program, query.as_deref())?;

        if let Some(function) = function {
            process.add_program(&program)?;
            ensure!(
                program.functions().contains_key(&function),
                "Function {} not found in program",
                function
            );

            let auth = process
                .get_stack(program.id())?
                .authorize::<N::Circuit, _>(
                    &PrivateKey::new(&mut rand::thread_rng())?,
                    function,
                    inputs.iter(),
                    &mut rand::thread_rng(),
                )?;

            estimate_cost(&process, &auth)
        } else {
            let deployment = process.deploy::<N::Circuit, _>(&program, &mut rand::thread_rng())?;
            Ok(deployment_cost(&deployment)?.0)
        }
    }
}
