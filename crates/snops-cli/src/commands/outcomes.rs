use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueHint};
use reqwest::blocking::{Client, Response};

/// For interacting with snop environments.
#[derive(Debug, Parser)]
pub struct Outcomes {
    #[clap(subcommand)]
    command: Commands,
}

/// Outcomes commands
#[derive(Debug, Parser)]
enum Commands {
    // Attach to an env or timeline unsure.
    // Maybe it requires a both?
    #[command(arg_required_else_help = true)]
    Attach {
        /// The env id.
        #[clap(value_hint = ValueHint::Other)]
        env_id: String,
        /// The name of the outcome.
        #[clap(value_hint = ValueHint::Other)]
        id: String,
    },
    #[command(arg_required_else_help = true)]
    Detach {
        /// The env id.
        #[clap(value_hint = ValueHint::Other)]
        env_id: String,
    },
    /// Prepare a outcome.
    #[command(arg_required_else_help = true)]
    Prepare {
        /// The name to give the outcome.
        #[clap(value_hint = ValueHint::Other)]
        id: String,
        /// The outcome file.
        #[clap(value_hint = ValueHint::AnyPath)]
        spec: PathBuf,
    },
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
            Attach { env_id, id } => {
                let ep = format!("{url}/api/v1/env/{env_id}/outcome/{id}");

                client.put(ep).send()?
            }
            Detach { env_id } => {
                let ep = format!("{url}/api/v1/env/{env_id}/outcome");

                client.delete(ep).send()?
            }
            Prepare { id, spec } => {
                let ep = format!("{url}/api/v1/outcome/{id}/prepare");
                let file: String = std::fs::read_to_string(spec)?;

                client.post(ep).body(file).send()?
            }
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
