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
    /// Clean a specific environment.
    #[command(arg_required_else_help = true)]
    Clean {
        /// Show a specific env.
        #[clap(value_hint = ValueHint::Other)]
        id: String,
    },
    /// List all environments.
    List,

    /// List all timelines for a specific environment.
    #[command(arg_required_else_help = true)]
    Timelines {
        /// Show a specific env.
        #[clap(value_hint = ValueHint::Other)]
        id: String,
    },

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
        env_id: String,
        /// Start a specific timeline.
        #[clap(value_hint = ValueHint::Other)]
        timeline_id: String,
    },

    /// Stop an environment's timeline.
    #[command(arg_required_else_help = true)]
    Stop {
        /// Stop a specific env.
        #[clap(value_hint = ValueHint::Other)]
        env_id: String,
        /// Stop a specific timeline.
        #[clap(value_hint = ValueHint::Other)]
        timeline_id: String,
    },
}

impl Env {
    pub fn run(self, url: &str, client: Client) -> Result<Response> {
        use Commands::*;
        Ok(match self.command {
            Clean { id } => {
                let ep = format!("{url}/api/v1/env/{id}");

                client.delete(ep).send()?
            }
            List => {
                let ep = format!("{url}/api/v1/env/list");

                client.get(ep).send()?
            }
            Timelines { id } => {
                let ep = format!("{url}/api/v1/env/{id}/timelines");

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
            Start {
                env_id,
                timeline_id,
            } => {
                let ep = format!("{url}/api/v1/env/{env_id}/{timeline_id}");

                client.post(ep).send()?
            }
            Stop {
                env_id,
                timeline_id,
            } => {
                let ep = format!("{url}/api/v1/env/{env_id}/{timeline_id}");

                client.delete(ep).send()?
            }
        })
    }
}
