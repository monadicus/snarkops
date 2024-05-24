// TODO should rename this file now...
use anyhow::{bail, Result};
use clap::Args;
use snarkvm::{
    console::program::{Locator, Network},
    synthesizer::cast_ref,
};

use super::fee::fee;
use crate::{runner::Key, use_process_downcast, Authorization, PTRecord, PrivateKey, Value};

#[derive(Clone, Debug, Args)]
pub struct Authorize<N: Network> {
    #[clap(flatten)]
    pub key: Key<N>,
    /// Program ID and function name (eg. credits.aleo/transfer_public)
    locator: Locator<N>,
    /// Program inputs (eg. 1u64 5field)
    #[clap(num_args = 1, value_delimiter = ' ')]
    inputs: Vec<Value<N>>,

    /// Enable additional fee execution step (optional)
    ///
    /// When not present, the authorization will not have a fee authorization.
    #[clap(long)]
    pub fee: bool,
    /// The priority fee in microcredits.
    #[clap(long, default_value_t = 0)]
    pub priority_fee: u64,
    /// The record for a private fee.
    #[clap(long)]
    pub record: Option<PTRecord<N>>,
}

impl<N: Network> Authorize<N> {
    /// Initializes a new authorization.
    pub fn parse(self) -> Result<Authorization<N>> {
        let private_key = self.key.try_get()?;
        let auth = use_process_downcast!(A, N, |process| {
            process.authorize::<A, _>(
                cast_ref!((private_key) as PrivateKey<N>),
                self.locator.program_id().to_string(),
                self.locator.resource().to_string(),
                cast_ref!((self.inputs) as Vec<Value<N>>).iter(),
                &mut rand::thread_rng(),
            )?
        });

        if !self.fee {
            return Ok(auth);
        }

        let Some(auth) = fee(
            auth,
            private_key,
            self.priority_fee,
            &mut rand::thread_rng(),
            self.record,
        )?
        else {
            bail!("Execution has no fee")
        };

        Ok(auth)
    }
}
