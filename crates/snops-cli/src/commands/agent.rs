use std::str::FromStr;

use anyhow::Result;
use clap::{error::ErrorKind, CommandFactory, Parser, ValueHint};
use reqwest::blocking::{Client, Response};
use snops_common::state::AgentId;

use super::DUMMY_ID;
use crate::Cli;

/// For interacting with snop agents.
#[derive(Debug, Parser)]
pub struct Agent {
    /// Show a specific agent's info.
    #[clap(value_hint = ValueHint::Other, default_value = DUMMY_ID)]
    id: AgentId,
    #[clap(subcommand)]
    command: AgentCommands,
}

/// Env commands
#[derive(Debug, Parser)]
enum AgentCommands {
    /// Get the specific agent.
    #[clap(alias = "i")]
    Info,

    /// List all agents.
    /// Ignores the agent id.
    #[clap(alias = "ls")]
    List,

    /// Get the specific agent's TPS.
    Tps,
}

impl Agent {
    pub fn run(self, url: &str, client: Client) -> Result<Response> {
        use AgentCommands::*;
        Ok(match self.command {
            List => {
                let ep = format!("{url}/api/v1/agents");

                client.get(ep).send()?
            }
            _ if self.id == AgentId::from_str(DUMMY_ID).unwrap() => {
                let mut cmd = Cli::command();
                cmd.error(
                    ErrorKind::MissingRequiredArgument,
                    " the following required arguments were not provided:\n  <ID>",
                )
                .exit();
            }
            Info => {
                let ep = format!("{url}/api/v1/agents/{}", self.id);

                client.get(ep).send()?
            }
            Tps => {
                let ep = format!("{url}/api/v1/agents/{}/tps", self.id);

                client.get(ep).send()?
            }
        })
    }
}
