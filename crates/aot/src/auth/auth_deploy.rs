use anyhow::{Ok, Result};
use clap::Args;
use clap_stdin::FileOrStdin;
use snarkvm::console::program::ProgramOwner;
use snarkvm::synthesizer::{Process, Program};

use super::args::AuthBlob;
use super::query;
use crate::runner::Key;
use crate::Network;

#[derive(Debug, Args)]
pub struct AuthDeployOptions<N: Network> {
    #[clap(short, long)]
    pub query: Option<String>,
    pub program: FileOrStdin<Program<N>>,
}

#[derive(Debug, Args)]
pub struct AuthorizeDeploy<N: Network> {
    #[clap(flatten)]
    pub key: Key<N>,
    #[clap(flatten)]
    pub options: AuthDeployOptions<N>,
}

impl<N: Network> AuthorizeDeploy<N> {
    pub fn parse(self) -> Result<AuthBlob<N>> {
        // get the program from the file (or stdin)
        let program = self.options.program.clone().contents()?;
        let mut process = Process::load()?;
        query::get_process_imports(&mut process, &program, self.options.query.as_deref())?;

        let rng = &mut rand::thread_rng();

        let deployment = process.deploy::<N::Circuit, _>(&program, rng)?;
        let deployment_id = deployment.to_deployment_id()?;

        let private_key = self.key.try_get()?;

        // Construct the owner.
        let owner = ProgramOwner::new(&private_key, deployment_id, rng)?;

        Ok(AuthBlob::Deploy {
            owner,
            deployment,
            fee_auth: None,
        })
    }
}
