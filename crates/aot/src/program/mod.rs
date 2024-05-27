use std::sync::OnceLock;

use anyhow::Result;
use clap::{Args, Subcommand};
use lazy_static::lazy_static;
use serde_json::json;
use snarkvm::{
    console::network::{MainnetV0, TestnetV0},
    synthesizer::Process,
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
    Execute(execute::Execute<N>),
    AuthorizeProgram(auth_program::AuthorizeProgram<N>),
    AuthorizeFee(auth_fee::AuthorizeFee<N>),
    Authorize(Authorize<N>),
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

impl<N: Network> Program<N> {
    pub(crate) fn parse(self) -> Result<()> {
        match self {
            Program::Execute(command) => command.parse(),
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
