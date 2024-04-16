use anyhow::Result;
use clap::{Parser, ValueHint};
use reqwest::blocking::{Client, Response};

/// For interacting with snop environments.
#[derive(Debug, Parser)]
pub struct Outcomes {
    #[clap(subcommand)]
    command: Commands,
}

/// Env commands
#[derive(Debug, Parser)]
enum Commands {
    /// Fetch a specific historic outcome.
    Get {
        /// The name of the outcome.
        #[clap(value_hint = ValueHint::Other)]
        id: String,
    },
    /// List all historic outcomes.
    List,
}

impl Outcomes {
    pub fn run(self, url: &str, client: Client) -> Result<Response> {
        use Commands::*;
        Ok(match self.command {
            Get { id } => {
                let ep = format!("{url}/api/v1/outcome/{id}");

                client.get(ep).send()?
            }
            List => {
                let ep = format!("{url}/api/v1/outcome/list");

                client.get(ep).send()?
            }
        })
    }
}
