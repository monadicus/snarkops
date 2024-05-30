use std::sync::OnceLock;

use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use lazy_static::lazy_static;
use serde_json::json;
use snarkvm::{
    console::{
        network::{MainnetV0, TestnetV0},
        types::Field,
    },
    synthesizer::{Authorization, Process},
};

use crate::{runner::Key, Network};

pub mod auth_fee;
pub mod auth_program;
pub mod execute;

lazy_static! {
    static ref PROCESS_MAINNET: OnceLock<Process<MainnetV0>> = Default::default();
    static ref PROCESS_TESTNET: OnceLock<Process<TestnetV0>> = Default::default();
}

/// Provide an Aleo and Network type based on the network ID, then return a
/// downcasted value back to the generic network...
#[macro_export]
macro_rules! mux_aleo {
    ($a:ident, $n:ident, $e:expr) => {
        *(match N::ID {
            <snarkvm::console::network::MainnetV0 as Network>::ID => {
                use anyhow::anyhow;
                type $a = snarkvm::circuit::AleoV0;
                type $n = snarkvm::console::network::MainnetV0;
                Box::new($e) as Box<dyn std::any::Any>
            }
            <snarkvm::console::network::TestnetV0 as Network>::ID => {
                use anyhow::anyhow;
                type $a = snarkvm::circuit::AleoTestnetV0;
                type $n = snarkvm::console::network::TestnetV0;
                Box::new($e) as Box<dyn std::any::Any>
            }
            _ => unreachable!(),
        })
        .downcast::<_>()
        .expect("Failed to downcast")
    };
}

/// Use the process for the network, then return a downcasted value back to the
/// generic network...
#[macro_export]
macro_rules! use_process_downcast {
    ($a:ident, $n:ident, |$process:ident| $e:expr) => {

        *(match N::ID {
            <snarkvm::console::network::MainnetV0 as Network>::ID => {
                use anyhow::anyhow;
                type $a = snarkvm::circuit::AleoV0;
                type $n = snarkvm::console::network::MainnetV0;
                let $process =
                $crate::program::PROCESS_MAINNET.get_or_init(|| snarkvm::synthesizer::Process::load().unwrap());
                Box::new($e) as Box<dyn std::any::Any>
            }
            <snarkvm::console::network::TestnetV0 as Network>::ID => {
                use anyhow::anyhow;
                type $a = snarkvm::circuit::AleoTestnetV0;
                type $n = snarkvm::console::network::TestnetV0;
                let $process =
                    $crate::program::PROCESS_TESTNET.get_or_init(|| snarkvm::synthesizer::Process::load().unwrap());
                Box::new($e) as Box<dyn std::any::Any>
            }
            _ => unreachable!(),
        })
        .downcast::<_>()
        .expect("Failed to downcast")
    };
}

/// Use the process for the network and return a non-network related value
#[macro_export]
macro_rules! use_process {
    ($a:ident, $n:ident, |$process:ident| $e:expr) => {
        match N::ID {
            <snarkvm::console::network::MainnetV0 as Network>::ID => {
                use anyhow::anyhow;
                type $a = snarkvm::circuit::AleoV0;
                type $n = snarkvm::console::network::MainnetV0;
                let $process =
                $crate::program::PROCESS_MAINNET.get_or_init(|| snarkvm::synthesizer::Process::load().unwrap());
                $e
            }
            <snarkvm::console::network::TestnetV0 as Network>::ID => {
                use anyhow::anyhow;
                type $a = snarkvm::circuit::AleoTestnetV0;
                type $n = snarkvm::console::network::TestnetV0;
                let $process =
                    $crate::program::PROCESS_TESTNET.get_or_init(|| snarkvm::synthesizer::Process::load().unwrap());
                $e
            }
            _ => unreachable!(),
        }
    };
}

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
    Id(AuthToId<N>),
}

#[derive(Debug, Args)]
pub struct Authorize<N: Network> {
    #[clap(flatten)]
    pub key: Key<N>,
    #[clap(flatten)]
    pub fee_opts: auth_fee::AuthFeeOptions<N>,
    #[clap(flatten)]
    pub program_opts: auth_program::AuthProgramOptions<N>,
}

#[derive(Debug, Args)]
pub struct AuthToId<N: Network> {
    #[clap(short, long)]
    pub auth: Authorization<N>,
    #[clap(short, long)]
    pub fee_auth: Option<Authorization<N>>,
}

// convert a fee authorization to a real (fake) fee :)
pub fn fee_from_auth<N: Network>(
    fee_auth: &Authorization<N>,
) -> Result<snarkvm::ledger::block::Fee<N>> {
    let Some(transition) = fee_auth.transitions().values().next().cloned() else {
        bail!("No transitions found in fee authorization");
    };
    snarkvm::ledger::block::Fee::from(transition, N::StateRoot::default(), None)
}

// compute the transaction ID for an authorization using the transitions and fee
// authorization
pub fn auth_tx_id<N: Network>(
    auth: &Authorization<N>,
    fee_auth: Option<&Authorization<N>>,
) -> Result<N::TransactionID> {
    let fee = fee_auth.map(fee_from_auth).transpose()?;

    let field: Field<N> =
        *snarkvm::ledger::block::Transaction::transitions_tree(auth.transitions().values(), &fee)?
            .root();

    Ok(field.into())
}

impl<N: Network> Program<N> {
    pub(crate) fn parse(self) -> Result<()> {
        match self {
            Program::Execute(command) => command.parse(),
            Program::Id(AuthToId { auth, fee_auth }) => {
                let id = auth_tx_id(&auth, fee_auth.as_ref())?;
                println!("{id}");
                Ok(())
            }
            Program::Authorize(Authorize {
                key,
                program_opts,
                fee_opts,
            }) => {
                let auth = auth_program::AuthorizeProgram {
                    key: key.clone(),
                    options: program_opts,
                }
                .parse()?;

                let fee_auth = auth_fee::AuthorizeFee {
                    key,
                    authorization: auth.clone(),
                    options: fee_opts,
                }
                .parse()?;

                println!(
                    "{}",
                    serde_json::to_string(&json!({
                        "auth": auth,
                        "fee_auth": fee_auth,
                    }))?
                );
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
