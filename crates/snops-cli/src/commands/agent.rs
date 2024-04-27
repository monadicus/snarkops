use anyhow::Result;
use clap::{error::ErrorKind, CommandFactory, Parser, ValueHint};
use reqwest::blocking::{Client, Response};

use crate::cli::Cli;

/// For interacting with snop environments.
#[derive(Debug, Parser)]
pub struct Agent {
    /// Show a specific agent's info.
    #[clap(value_hint = ValueHint::Other, default_value = "")]
    id: String,
    #[clap(subcommand)]
    command: Commands,
}

/// Env commands
#[derive(Debug, Parser)]
enum Commands {
    /// Get the specific agent.
    Info,

    /// List all agents.
    /// Ignores the agent id.
    List,

    /// Get the specific agent's TPS.
    Tps,
}

impl Agent {
    pub fn run(self, url: &str, client: Client) -> Result<Response> {
        use Commands::*;
        Ok(match self.command {
            List => {
                let ep = format!("{url}/api/v1/agents");

                client.get(ep).send()?
            }
            _ if self.id.is_empty() => {
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
