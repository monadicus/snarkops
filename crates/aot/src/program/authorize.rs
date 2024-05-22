// TODO should rename this file now...
use anyhow::Result;
use clap::Args;
use snarkvm::{console::program::Network, synthesizer::cast_ref};

use crate::{mux_process, Authorization, PrivateKey, Value};

#[derive(Clone, Debug, Args)]
pub struct Authorize<N: Network> {
    #[clap(long)]
    private_key: PrivateKey<N>,
    #[clap(long)]
    program_id: String,
    #[clap(long)]
    function_name: String,
    #[clap(long)]
    inputs: Vec<Value<N>>,
}

impl<N: Network> Authorize<N> {
    /// Initializes a new authorization.
    pub fn parse(self) -> Result<Authorization<N>> {
        Ok(mux_process!(A, N, |process| {
            process.authorize::<A, _>(
                cast_ref!((self.private_key) as PrivateKey<N>),
                self.program_id,
                self.function_name,
                cast_ref!((self.inputs) as Vec<Value<N>>).iter(),
                &mut rand::thread_rng(),
            )?
        }))
    }
}
