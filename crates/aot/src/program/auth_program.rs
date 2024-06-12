use anyhow::{anyhow, Result};
use clap::Args;
use snarkvm::{
    console::program::{Locator, Network},
    synthesizer::cast_ref,
};

use crate::{runner::Key, use_process_downcast, Authorization, PrivateKey, Value};

#[derive(Debug, Args)]
pub struct AuthProgramOptions<N: Network> {
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
}

impl<N: Network> AuthorizeProgram<N> {
    /// Initializes a new authorization.
    pub fn parse(self) -> Result<Authorization<N>> {
        let private_key = self.key.try_get()?;
        let auth = use_process_downcast!(A, N, |process| {
            process.authorize::<A, _>(
                cast_ref!((private_key) as PrivateKey<N>),
                self.options.locator.program_id().to_string(),
                self.options.locator.resource().to_string(),
                cast_ref!((self.options.inputs) as Vec<Value<N>>).iter(),
                &mut rand::thread_rng(),
            )?
        });

        Ok(auth)
    }
}
