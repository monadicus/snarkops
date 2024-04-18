use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueHint};
use reqwest::blocking::{Client, Response};

/// For interacting with snop environments.
#[derive(Debug, Parser)]
pub struct Env {
    #[clap(subcommand)]
    command: Commands,
}

/// Env commands
#[derive(Debug, Parser)]
enum Commands {
    /// List all environments.
    List,

    /// Show the current topology of a specific environment.
    #[command(arg_required_else_help = true)]
    Topology {
        /// Show a specific env.
        #[clap(value_hint = ValueHint::Other)]
        id: String,
    },

    /// Prepare a (test) environment.
    #[command(arg_required_else_help = true)]
    Prepare {
        id: String,
        /// The test spec file.
        #[clap(value_hint = ValueHint::AnyPath)]
        spec: PathBuf,
    },

    /// Start an environment's timeline (a test).
    #[command(arg_required_else_help = true)]
    Start {
        /// Start a specific env.
        #[clap(value_hint = ValueHint::Other)]
        id: String,
    },

    /// Stop an environment's timeline.
    #[command(arg_required_else_help = true)]
    Stop {
        /// Stop a specific env.
        #[clap(value_hint = ValueHint::Other)]
        id: String,
    },
}

impl Env {
    pub fn run(self, url: &str, client: Client) -> Result<Response> {
        use Commands::*;
        Ok(match self.command {
            List => {
                let ep = format!("{url}/api/v1/env/list");

                client.get(ep).send()?
            }
            Topology { id } => {
                let ep = format!("{url}/api/v1/env/{id}/topology");

                client.get(ep).send()?
            }
            Prepare { id, spec } => {
                let ep = format!("{url}/api/v1/env/{id}/prepare");
                let file: String = std::fs::read_to_string(spec)?;

                client.post(ep).body(file).send()?
            }
            Start { id } => {
                let ep = format!("{url}/api/v1/env/{id}");

                client.post(ep).send()?
            }
            Stop { id } => {
                let ep = format!("{url}/api/v1/env/{id}");

                client.delete(ep).send()?
            }
        })
    }
}
