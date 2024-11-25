use anyhow::{bail, Result};
use clap::Args;
use snarkvm::{console::program::Locator, synthesizer::Process};

use super::{auth_fee::estimate_cost, query};
use crate::{Authorization, Key, Network, Value};

#[derive(Debug, Args)]
pub struct AuthProgramOptions<N: Network> {
    /// Query to load the program with.
    #[clap(env, short, long)]
    pub query: Option<String>,
    /// Program ID and function name (eg. credits.aleo/transfer_public)
    locator: Locator<N>,
    /// Program inputs (eg. 1u64 5field)
    #[clap(num_args = 1, value_delimiter = ' ')]
    inputs: Vec<Value<N>>,
}

#[derive(Debug, Args)]
pub struct AuthorizeProgram<N: Network> {
    #[clap(flatten)]
    pub key: Key<N>,
    #[clap(flatten)]
    pub options: AuthProgramOptions<N>,
    /// The seed to use for the authorization generation
    #[clap(long)]
    pub seed: Option<u64>,
}

impl<N: Network> AuthorizeProgram<N> {
    /// Initializes a new authorization.
    pub fn parse(self) -> Result<(Authorization<N>, u64)> {
        let private_key = self.key.try_get()?;

        let mut process = Process::load()?;
        match (self.options.query, self.options.locator.program_id()) {
            (_, id) if *id == N::credits() => {}
            (None, id) => {
                bail!("Query required to authorize non-credits program {}", id);
            }
            (Some(query), id) => query::load_program(&mut process, *id, &query)?,
        };

        let auth = process
            .get_stack(self.options.locator.program_id())?
            .authorize::<N::Circuit, _>(
                &private_key,
                self.options.locator.resource(),
                self.options.inputs.iter(),
                &mut super::rng_from_seed(self.seed),
            )?;

        let cost = estimate_cost(&process, &auth)?;

        Ok((auth, cost))
    }
}
