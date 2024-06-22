use anyhow::Result;
use args::{AuthArgs, AuthBlob, FeeKey};
use auth_fee::estimate_cost;
use clap::{Args, Subcommand};
use snarkvm::synthesizer::{process::deployment_cost, Process};

use crate::{Key, Network};

pub mod args;
pub mod auth_deploy;
pub mod auth_fee;
pub mod auth_id;
pub mod auth_program;
pub mod execute;
pub mod query;

/// A command to help generate various different types of authorizations and
/// execute them.
#[derive(Debug, Subcommand)]
pub enum AuthCommand<N: Network> {
    Execute(execute::Execute<N>),
    Program(AuthProgramCommand<N>),
    Fee(auth_fee::AuthorizeFee<N>),
    /// Given an authorization (and fee), return the transaction ID.
    Id(AuthArgs<N>),
    Cost(CostCommand<N>),
    Deploy(AuthDeployCommand<N>),
}

/// Estimate the cost of a program execution or deployment.

#[derive(Debug, Args)]
pub struct CostCommand<N: Network> {
    /// The query to use for the program.
    #[clap(long)]
    query: Option<String>,
    #[clap(flatten)]
    auth: AuthArgs<N>,
}

/// Authorize a program execution.
#[derive(Debug, Args)]
pub struct AuthProgramCommand<N: Network> {
    #[clap(flatten)]
    pub key: Key<N>,
    #[clap(flatten)]
    pub fee_key: FeeKey<N>,
    /// Prevent the fee from being included in the authorization
    #[clap(long)]
    pub skip_fee: bool,
    #[clap(flatten)]
    pub fee_opts: auth_fee::AuthFeeOptions<N>,
    #[clap(flatten)]
    pub program_opts: auth_program::AuthProgramOptions<N>,
}

/// Deploy a program to the network.
#[derive(Debug, Args)]
pub struct AuthDeployCommand<N: Network> {
    #[clap(flatten)]
    pub key: Key<N>,
    #[clap(flatten)]
    pub fee_key: FeeKey<N>,
    /// Prevent the fee from being included in the authorization
    #[clap(long)]
    pub skip_fee: bool,
    #[clap(flatten)]
    pub fee_opts: auth_fee::AuthFeeOptions<N>,
    #[clap(flatten)]
    pub deploy_opts: auth_deploy::AuthDeployOptions<N>,
}

impl<N: Network> AuthCommand<N> {
    pub(crate) fn parse(self) -> Result<()> {
        match self {
            // execute command consumes authorizations and outputs a transaction
            AuthCommand::Execute(command) => command.parse(),
            // fee command consumes an authorization and a private key to pay a fee. outputs a fee
            // authorization
            AuthCommand::Fee(fee) => {
                println!("{}", serde_json::to_string(&fee.parse()?)?);
                Ok(())
            }
            // id command consumes an authorization or deployment and outputs the predicted
            // transaction id
            AuthCommand::Id(args) => {
                let id = match args.pick()? {
                    AuthBlob::Program { auth, fee_auth } => {
                        let auth = auth.into();
                        let fee_auth = fee_auth.map(Into::into);

                        // calculate the transaction id for the program based off of the authorization and fee
                        auth_id::auth_tx_id(&auth, fee_auth.as_ref())?
                    }
                    AuthBlob::Deploy {
                        deployment,
                        fee_auth,
                        ..
                        // calculate the transaction id for the deployment based off of the deployment and fee
                    } => auth_id::deploy_tx_id(&deployment, fee_auth.map(Into::into).as_ref())?,
                };
                println!("{id}");
                Ok(())
            }
            AuthCommand::Cost(CostCommand { query, auth }) => {
                let cost = match auth.pick()? {
                    AuthBlob::Program { auth, .. } => {
                        let auth = auth.into();

                        // load the programs the auth references into the process
                        // as cost estimation measures the size of values from within the auth's
                        // transitions
                        let mut process = Process::load()?;
                        if let Some(query) = query.as_deref() {
                            let programs = query::get_programs_from_auth(&auth);
                            query::add_many_programs_to_process(&mut process, programs, query)?;
                        }

                        estimate_cost(&process, &auth)?
                    }
                    AuthBlob::Deploy { deployment, .. } => deployment_cost(&deployment)?.0,
                };
                println!("{cost}");
                Ok(())
            }
            AuthCommand::Program(AuthProgramCommand {
                key,
                skip_fee,
                fee_key,
                program_opts,
                fee_opts,
            }) => {
                let query = program_opts.query.clone();

                // authorize the program execution without a fee
                let (auth, cost) = auth_program::AuthorizeProgram {
                    key: key.clone(),
                    options: program_opts,
                }
                .parse()?;

                if skip_fee {
                    println!("{}", serde_json::to_string(&auth)?);
                    return Ok(());
                };

                // authorize the fee using the authorization's execution ID and estimated cost
                let fee_auth = auth_fee::AuthorizeFee {
                    key: fee_key.as_key().unwrap_or(key),
                    auth: None,
                    options: fee_opts,
                    deployment: None,
                    query,
                    id: Some(auth.to_execution_id()?),
                    cost: Some(cost),
                }
                .parse()?;

                println!(
                    "{}",
                    serde_json::to_string(&AuthBlob::Program {
                        auth: auth.into(),
                        fee_auth: fee_auth.map(Into::into)
                    })?
                );
                Ok(())
            }
            AuthCommand::Deploy(AuthDeployCommand {
                key,
                skip_fee,
                fee_key,
                deploy_opts,
                fee_opts,
            }) => {
                // authorize the deployment without a fee
                let AuthBlob::Deploy {
                    deployment, owner, ..
                } = auth_deploy::AuthorizeDeploy {
                    key: key.clone(),
                    options: deploy_opts,
                }
                .parse()?
                else {
                    unreachable!("authorize deploy never returns a program auth")
                };

                if skip_fee {
                    println!(
                        "{}",
                        serde_json::to_string(&AuthBlob::Deploy {
                            deployment,
                            owner,
                            fee_auth: None
                        })?
                    );
                    return Ok(());
                };

                // authorize the fee using the deployment's ID and estimated cost
                let fee_auth = auth_fee::AuthorizeFee {
                    key: fee_key.as_key().unwrap_or(key),
                    auth: None,
                    options: fee_opts,
                    deployment: None,
                    query: None,
                    id: Some(deployment.to_deployment_id()?),
                    cost: Some(deployment_cost(&deployment)?.0),
                }
                .parse()?
                .map(Into::into);

                println!(
                    "{}",
                    serde_json::to_string(&AuthBlob::Deploy {
                        deployment,
                        fee_auth,
                        owner,
                    })?
                );
                Ok(())
            }
        }
    }
}
