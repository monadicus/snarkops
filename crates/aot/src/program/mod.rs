use anyhow::Result;
use args::{AuthArgs, AuthBlob, FeeKey};
use clap::{Args, Subcommand};

use crate::{runner::Key, Network};

pub mod args;
pub mod auth_fee;
pub mod auth_id;
pub mod auth_program;
pub mod execute;
mod macros;
pub use macros::*;

#[derive(Debug, Subcommand)]
pub enum Program<N: Network> {
    /// Execute an authorization
    Execute(execute::Execute<N>),
    /// Authorize a program execution
    AuthorizeProgram(auth_program::AuthorizeProgram<N>),
    /// Authorize the fee for a program execution
    AuthorizeFee(auth_fee::AuthorizeFee<N>),
    /// Authorize a program execution and its fee
    Authorize(Authorize<N>),
    /// Given an authorization (and fee), return the transaction ID
    Id(AuthArgs<N>),
}

#[derive(Debug, Args)]
pub struct Authorize<N: Network> {
    #[clap(flatten)]
    pub key: Key<N>,
    #[clap(flatten)]
    pub fee_key: FeeKey<N>,
    #[clap(flatten)]
    pub fee_opts: auth_fee::AuthFeeOptions<N>,
    #[clap(flatten)]
    pub program_opts: auth_program::AuthProgramOptions<N>,
}

impl<N: Network> Program<N> {
    pub(crate) fn parse(self) -> Result<()> {
        match self {
            Program::Execute(command) => command.parse(),
            Program::Id(args) => {
                let AuthBlob { auth, fee_auth } = args.pick()?;
                let id = auth_id::auth_tx_id(&auth, fee_auth.as_ref())?;
                println!("{id}");
                Ok(())
            }
            Program::Authorize(Authorize {
                key,
                fee_key,
                program_opts,
                fee_opts,
            }) => {
                let auth = auth_program::AuthorizeProgram {
                    key: key.clone(),
                    options: program_opts,
                }
                .parse()?;

                let fee_auth = auth_fee::AuthorizeFee {
                    key: fee_key.as_key().unwrap_or(key),
                    auth: auth.clone(),
                    options: fee_opts,
                }
                .parse()?;

                println!("{}", serde_json::to_string(&AuthBlob { auth, fee_auth })?);
                Ok(())
            }
            Program::AuthorizeProgram(command) => {
                println!("{}", serde_json::to_string(&command.parse()?)?);
                Ok(())
            }
            Program::AuthorizeFee(command) => {
                println!("{}", serde_json::to_string(&command.parse()?)?);
                Ok(())
            }
        }
    }
}
