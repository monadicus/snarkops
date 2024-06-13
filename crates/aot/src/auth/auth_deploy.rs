use std::collections::{HashMap, VecDeque};
use std::str::FromStr;

use anyhow::{anyhow, bail, Ok, Result};
use clap::Args;
use clap_stdin::FileOrStdin;
use snarkvm::console::program::ProgramOwner;
use snarkvm::prelude::ProgramID;
use snarkvm::synthesizer::{Process, Program, Stack};

use super::args::AuthBlob;
use crate::runner::Key;
use crate::Network;

#[derive(Debug, Args)]
pub struct AuthDeployOptions<N: Network> {
    #[clap(short, long, group = "deploy")]
    pub query: Option<String>,
    #[clap(group = "deploy")]
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
    /// Fetches a program from the query endpoint.
    async fn fetch_program(&self, id: ProgramID<N>) -> Result<Program<N>> {
        if let Some(query) = &self.options.query {
            Ok(reqwest::get(format!("{query}/program/{id}"))
                .await?
                .json()
                .await?)
        } else {
            bail!("no query endpoint provided, cannot fetch program. Local file cache not implemented")
        }
    }

    /// Walks the program's imports and fetches them all.
    async fn get_imports(&self, program: &Program<N>) -> Result<HashMap<ProgramID<N>, Program<N>>> {
        let credits = ProgramID::<N>::from_str("credits.aleo").unwrap();

        let mut imported = HashMap::new();
        let mut queue = VecDeque::new();
        queue.push_back(program.clone());

        while let Some(program) = queue.pop_front() {
            for import in program.imports().keys() {
                if imported.contains_key(import) || import == &credits {
                    continue;
                }

                let import_program = self
                    .fetch_program(*import)
                    .await
                    .map_err(|e| anyhow!("failed to fetch imported program {import}: {e:?}"))?;

                imported.insert(*import, import_program.clone());
                queue.push_back(import_program);
            }
        }

        Ok(imported)
    }

    #[tokio::main]
    pub async fn parse(self) -> Result<AuthBlob<N>> {
        // get the program from the file (or stdin)
        let program = self.options.program.clone().contents()?;
        let imports = self.get_imports(&program).await?;

        let rng = &mut rand::thread_rng();

        let mut process = Process::load()?;

        for (_, import) in imports {
            process.add_stack(Stack::new(&process, &import)?);
        }

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
