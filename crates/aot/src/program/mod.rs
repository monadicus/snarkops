use std::sync::OnceLock;

use anyhow::Result;
use clap::Subcommand;
use lazy_static::lazy_static;
use snarkvm::{
    console::network::{MainnetV0, TestnetV0},
    synthesizer::Process,
};

use crate::Network;

pub mod authorize;
pub mod execute;
pub mod fee;

lazy_static! {
    static ref PROCESS_MAINNET: OnceLock<Process<MainnetV0>> = Default::default();
    static ref PROCESS_TESTNET: OnceLock<Process<TestnetV0>> = Default::default();
}

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

#[macro_export]
macro_rules! mux_process {
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

#[derive(Debug, Subcommand)]
pub enum Program<N: Network> {
    Execute(execute::Execute<N>),
    Authorize(authorize::Authorize<N>),
    AuthorizeFee(fee::AuthorizeFee<N>),
}
impl<N: Network> Program<N> {
    pub(crate) fn parse(self) -> Result<()> {
        match self {
            Program::Execute(command) => command.parse(),
            Program::Authorize(command) => {
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
