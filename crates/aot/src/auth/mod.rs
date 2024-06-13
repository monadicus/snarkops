use anyhow::Result;
use args::{AuthArgs, AuthBlob, FeeKey};
use clap::{Args, Subcommand};

use crate::{runner::Key, Network};

pub mod args;
pub mod auth_deploy;
pub mod auth_fee;
pub mod auth_id;
pub mod auth_program;
pub mod execute;

#[derive(Debug, Subcommand)]
pub enum AuthCommand<N: Network> {
    /// Execute an authorization
    Execute(execute::Execute<N>),
    /// Authorize a program execution
    Program(AuthProgramCommand<N>),
    /// Authorize the fee for a program execution
    Fee(auth_fee::AuthorizeFee<N>),
    /// Given an authorization (and fee), return the transaction ID
    Id(AuthArgs<N>),
    /// Deploy a program to the network
    Deploy(AuthDeployCommand<N>),
}

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
            AuthCommand::Execute(command) => command.parse(),
            AuthCommand::Fee(fee) => {
                println!("{}", serde_json::to_string(&fee.parse()?)?);
                Ok(())
            }
            AuthCommand::Id(args) => {
                let id = match args.pick()? {
                    AuthBlob::Program { auth, fee_auth } => {
                        auth_id::auth_tx_id(&auth, fee_auth.as_ref())?
                    }
                    AuthBlob::Deploy {
                        deployment,
                        fee_auth,
                        ..
                    } => auth_id::deploy_tx_id(&deployment, fee_auth.as_ref())?,
                };
                println!("{id}");
                Ok(())
            }
            AuthCommand::Program(AuthProgramCommand {
                key,
                skip_fee,
                fee_key,
                program_opts,
                fee_opts,
            }) => {
                let auth = auth_program::AuthorizeProgram {
                    key: key.clone(),
                    options: program_opts,
                }
                .parse()?;

                if skip_fee {
                    println!("{}", serde_json::to_string(&auth)?);
                    return Ok(());
                };

                let fee_auth = auth_fee::AuthorizeFee {
                    key: fee_key.as_key().unwrap_or(key),
                    auth: Some(auth.clone()),
                    options: fee_opts,
                    deployment: None,
                    id: None,
                    cost: None,
                }
                .parse()?;

                println!(
                    "{}",
                    serde_json::to_string(&AuthBlob::Program { auth, fee_auth })?
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

                let fee_auth = auth_fee::AuthorizeFee {
                    key: fee_key.as_key().unwrap_or(key),
                    auth: None,
                    options: fee_opts,
                    deployment: Some(deployment.clone()),
                    id: None,
                    cost: None,
                }
                .parse()?;

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
