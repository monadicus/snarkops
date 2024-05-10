// TODO should rename this file now...
use anyhow::Result;
use clap::Args;

use super::PROCESS;
use crate::{Aleo, Authorization, PrivateKey, Value};

#[derive(Clone, Debug, Args)]
pub struct Authorize {
    #[clap(long)]
    private_key: PrivateKey,
    #[clap(long)]
    program_id: String,
    #[clap(long)]
    function_name: String,
    #[clap(long)]
    inputs: Vec<Value>,
}

impl Authorize {
    /// Initializes a new authorization.
    pub fn parse(self) -> Result<Authorization> {
        let function = PROCESS.authorize::<Aleo, _>(
            &self.private_key,
            self.program_id,
            self.function_name,
            self.inputs.into_iter(),
            &mut rand::thread_rng(),
        )?;

        // Construct the authorization.
        Ok(function)
    }
}
