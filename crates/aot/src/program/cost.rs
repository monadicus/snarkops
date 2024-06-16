use anyhow::Result;
use clap::Args;
use clap_stdin::FileOrStdin;
use snarkvm::{
    prelude::{Identifier, Value},
    synthesizer::{Process, Program},
};

use crate::{
    auth::{auth_fee::estimate_cost, query},
    Network, PrivateKey,
};

#[derive(Debug, Args)]
pub struct CostCommand<N: Network> {
    /// Query to load the program with.
    #[clap(short, long)]
    pub query: Option<String>,
    /// Program to estimate the cost of
    pub program: FileOrStdin<Program<N>>,
    /// Program ID and function name (eg. credits.aleo/transfer_public)
    function: Identifier<N>,
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
        process.add_program(&program)?;
        query::get_process_imports(&mut process, &program, query.as_deref())?;

        let auth = process
            .get_stack(program.id())?
            .authorize::<N::Circuit, _>(
                &PrivateKey::new(&mut rand::thread_rng())?,
                function,
                inputs.iter(),
                &mut rand::thread_rng(),
            )?;

        let cost = estimate_cost(&process, &auth)?;
        Ok(cost)
    }
}
