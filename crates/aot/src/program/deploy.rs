use std::collections::{HashMap, VecDeque};
use std::str::FromStr;

use anyhow::{anyhow, bail, Ok, Result};
use clap::Args;
use clap_stdin::FileOrStdin;
use snarkvm::console::account::PrivateKey;
use snarkvm::console::program::ProgramOwner;
use snarkvm::ledger::query::Query;
use snarkvm::ledger::store::helpers::memory::ConsensusMemory;
use snarkvm::ledger::store::ConsensusStore;
use snarkvm::prelude::ProgramID;
use snarkvm::synthesizer::process::deployment_cost;
use snarkvm::synthesizer::{cast_ref, Process, Program, Stack};

use crate::runner::Key;
use crate::{MemVM, Network, Transaction};

#[derive(Debug, Args)]
pub struct Deploy<N: Network> {
    #[clap(flatten)]
    pub private_key: Key<N>,
    #[clap(short, long, default_value_t = true)]
    pub execute: bool,
    #[clap(short, long)]
    pub query: Option<String>,
    #[clap(short, long, default_value_t = 0)]
    pub priority_fee: u64,
    pub program: FileOrStdin<Program<N>>,
}

impl<N: Network> Deploy<N> {
    /// Fetches a program from the query endpoint.
    async fn fetch_program(&self, id: ProgramID<N>) -> Result<Program<N>> {
        if let Some(query) = &self.query {
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
    pub async fn parse(self) -> Result<()> {
        // get the program from the file (or stdin)
        let program = self.program.clone().contents()?;
        let imports = self.get_imports(&program).await?;

        let rng = &mut rand::thread_rng();

        let mut process = Process::load()?;

        for (_, import) in imports {
            process.add_stack(Stack::new(&process, &import)?);
        }

        let deployment = process.deploy::<N::Circuit, _>(&program, rng)?;
        let deployment_id = deployment.to_deployment_id()?;

        // Compute the minimum deployment cost.
        let (minimum_deployment_cost, _) = deployment_cost(&deployment)?;

        let private_key = self.private_key.try_get()?;

        // Prepare the fees.
        // let fee = match &self.record {
        //     Some(record) => {
        //         let fee_authorization = vm.authorize_fee_private(
        //             &private_key,
        //             fee_record,
        //             minimum_deployment_cost,
        //             self.priority_fee,
        //             deployment_id,
        //             rng,
        //         )?;
        //         vm.execute_fee_authorization(fee_authorization, Some(query), rng)?
        //     }
        //     None => {
        //         let fee_authorization = vm.authorize_fee_public(
        //             &private_key,
        //             minimum_deployment_cost,
        //             self.priority_fee,
        //             deployment_id,
        //             rng,
        //         )?;
        //         vm.execute_fee_authorization(fee_authorization, Some(query), rng)?
        //         }
        //     };

        let fee = {
            let store = ConsensusStore::<N, ConsensusMemory<_>>::open(None)?;
            let vm = MemVM::from(store)?;
            let fee_authorization = vm.authorize_fee_public(
                &private_key,
                minimum_deployment_cost,
                self.priority_fee,
                deployment_id,
                rng,
            )?;
            vm.execute_fee_authorization(fee_authorization, self.query.map(Query::REST), rng)?
        };
        // Construct the owner.

        let owner = ProgramOwner::new(&private_key, deployment_id, rng)?;
        let tx = Transaction::from_deployment(owner, deployment, fee)?;

        // Determine if the transaction should be broadcast, stored, or displayed to the
        // user.

        // Developer::handle_transaction(&self.broadcast,
        // self.dry_run, &self.store, transaction, program_id.to_string())
        Ok(())
    }
}
