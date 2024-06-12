use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{anyhow, Ok, Result};
use clap::Args;
use snarkvm::console::account::PrivateKey;
use snarkvm::console::program::{Network, ProgramOwner};
use snarkvm::file::AleoFile;
use snarkvm::ledger::Deployment;
use snarkvm::prelude::{Field, ProgramID};
use snarkvm::synthesizer::cast_ref;
use snarkvm::synthesizer::process::deployment_cost;

use crate::runner::Key;
use crate::{use_process_mut, Transaction};

#[derive(Debug, Args)]
pub struct Deploy<N: Network> {
    #[arg(short, long)]
    pub id: ProgramID<N>,
    #[clap(flatten)]
    pub private_key: Key<N>,
    #[clap(short, long, default_value_t = true)]
    pub execute: bool,
    #[clap(short, long)]
    pub program: PathBuf,
}

impl<N: Network> Deploy<N> {
    pub fn parse(self) -> Result<()> {
        // TODO - fetch these automatically :)
        let mut imports_directory = self.program.clone();
        imports_directory.pop();
        imports_directory.push("imports");

        // get the program from the file
        let program_str = std::fs::read_to_string(self.program)?;

        let rng = &mut rand::thread_rng();

        use_process_mut!(A, N, |process| {
            let aleo_file = AleoFile::<N>::from_str(&program_str)?;
            let program = aleo_file.program();
            // let program_id = program.id();
            let credits_program_id = ProgramID::<N>::from_str("credits.aleo")?;

            program.imports().keys().try_for_each(|program_id| {
                // Don't add `credits.aleo` as the process is already loaded with it.
                if program_id != &credits_program_id {
                    // TODO this is where we would fetch the program
                    // Open the Aleo program file.
                    let import_program_file =
                        AleoFile::open(&imports_directory, program_id, false)?;
                    // Add the import program.
                    process.add_program(import_program_file.program())?;
                }
                Ok(())
            })?;

            let deployment: Deployment<N> = process.deploy::<A, _>(program, rng)?;
            let deployment_id: Field<N> = deployment.to_deployment_id()?;

            // Compute the minimum deployment cost.
            let (minimum_deployment_cost, (_, _, _)) = deployment_cost(&deployment)?;

            // Construct the owner.
            let key = self.private_key.try_get()?;
            let private_key = cast_ref!(key as PrivateKey<N>);
            let owner = ProgramOwner::<N>::new(private_key, deployment_id, rng)?;

            // Prepare the fees.
            let fee = match &self.record {
                Some(record) => {
                    let fee_authorization = vm.authorize_fee_private(
                        &private_key,
                        fee_record,
                        minimum_deployment_cost,
                        self.priority_fee,
                        deployment_id,
                        rng,
                    )?;
                    vm.execute_fee_authorization(fee_authorization, Some(query), rng)?
                }
                None => {
                    let fee_authorization = vm.authorize_fee_public(
                        &private_key,
                        minimum_deployment_cost,
                        self.priority_fee,
                        deployment_id,
                        rng,
                    )?;
                    vm.execute_fee_authorization(fee_authorization, Some(query), rng)?
                }
            };

            let tx = Transaction::from_deployment(owner, deployment, None)?;

            // Determine if the transaction should be broadcast, stored, or displayed to the
            // user.

            // Developer::handle_transaction(&self.broadcast,
            // self.dry_run, &self.store, transaction, program_id.to_string())
            Ok(())
        })?;

        Ok(())
    }
}
