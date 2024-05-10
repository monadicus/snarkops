use anyhow::Result;
use clap::Subcommand;
use snarkvm::synthesizer::Process;

use crate::Network;

pub mod authorize;
pub mod execute;
pub mod fee;

lazy_static::lazy_static! {
    /// The main process.
    pub(crate) static ref PROCESS: Process<Network> = Process::load().unwrap();
}

#[derive(Debug, Subcommand)]
pub enum Program {
    Execute(execute::Execute),
    Authorize(authorize::Authorize),
    AuthorizeFee(fee::AuthorizeFee),
}
impl Program {
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
