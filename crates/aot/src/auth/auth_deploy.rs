use anyhow::{Ok, Result};
use clap::Args;
use clap_stdin::FileOrStdin;
use snarkvm::console::program::ProgramOwner;
use snarkvm::synthesizer::{Process, Program};

use super::args::AuthBlob;
use super::query;
use crate::Key;
use crate::Network;

/// Options for authorizing a program deployment.
#[derive(Debug, Args)]
pub struct AuthDeployOptions<N: Network> {
    /// The query to use for the program.
    #[clap(short, long)]
    pub query: Option<String>,
    /// The program to deploy.
    /// This can be a file or stdin.
    pub program: FileOrStdin<Program<N>>,
}

#[derive(Debug, Args)]
pub struct AuthorizeDeploy<N: Network> {
    #[clap(flatten)]
    pub key: Key<N>,
    #[clap(flatten)]
    pub options: AuthDeployOptions<N>,
    /// The seed to use for the authorization generation
    #[clap(long)]
    pub seed: Option<u64>,
}

impl<N: Network> AuthorizeDeploy<N> {
    pub fn parse(self) -> Result<AuthBlob<N>> {
        // get the program from the file (or stdin)
        let program = self.options.program.clone().contents()?;
        let mut process = Process::load()?;
        query::get_process_imports(&mut process, &program, self.options.query.as_deref())?;

        let deployment =
            process.deploy::<N::Circuit, _>(&program, &mut super::rng_from_seed(self.seed))?;
        let deployment_id = deployment.to_deployment_id()?;

        let private_key = self.key.try_get()?;

        // Construct the owner.
        let owner = ProgramOwner::new(
            &private_key,
            deployment_id,
            &mut super::rng_from_seed(self.seed),
        )?;

        Ok(AuthBlob::Deploy {
            owner,
            deployment,
            fee_auth: None,
        })
    }
}
