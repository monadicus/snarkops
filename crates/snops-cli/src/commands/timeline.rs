use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueHint};
use reqwest::blocking::{Client, Response};

/// For interacting with snop environments.
#[derive(Debug, Parser)]
pub struct Timeline {
    #[clap(subcommand)]
    command: Commands,
}

/// Env commands
#[derive(Debug, Parser)]
enum Commands {
    /// Attach to an environment.
    #[command(arg_required_else_help = true)]
    Attach {
        /// The env id.
        #[clap(value_hint = ValueHint::Other)]
        env_id: String,
        /// The name of the timeline.
        #[clap(value_hint = ValueHint::Other)]
        id: String,
    },
    /// Detatch a timeline from an environment.
    #[command(arg_required_else_help = true)]
    Detach {
        /// The env id.
        #[clap(value_hint = ValueHint::Other)]
        env_id: String,
    },
    /// Fetch a specific timeline.
    Get {
        /// The name of the timeline.
        #[clap(value_hint = ValueHint::Other)]
        id: String,
    },
    /// List all timelines.
    List,
    /// Prepare a timeline.
    #[command(arg_required_else_help = true)]
    Prepare {
        /// The name to give the timeline.
        #[clap(value_hint = ValueHint::Other)]
        id: String,
        /// The timeline file.
        #[clap(value_hint = ValueHint::AnyPath)]
        spec: PathBuf,
    },
    /// Start/Resume the timeline for an environment.
    #[command(arg_required_else_help = true)]
    Start {
        /// The id of the env.
        #[clap(value_hint = ValueHint::Other)]
        env_id: String,
    },
    /// Step through the timeline for an environment.
    #[command(arg_required_else_help = true)]
    Step {
        /// The id of the env.
        #[clap(value_hint = ValueHint::Other)]
        env_id: String,

        /// The number of steps to take.
        #[clap(long, short, default_value_t = 1)]
        num: u64,
    },
    /// Stop/Pause the timeline for an environment.
    #[command(arg_required_else_help = true)]
    Stop {
        /// The id of the env.
        #[clap(value_hint = ValueHint::Other)]
        env_id: String,
    },
}

impl Timeline {
    pub fn run(self, url: &str, client: Client) -> Result<Response> {
        use Commands::*;
        Ok(match self.command {
            Attach { env_id, id } => {
                let ep = format!("{url}/api/v1/env/{env_id}/timeline/{id}");

                client.put(ep).send()?
            }
            Detach { env_id } => {
                let ep = format!("{url}/api/v1/env/{env_id}/timeline");

                client.delete(ep).send()?
            }
            Get { id } => {
                let ep = format!("{url}/api/v1/timeline/{id}");

                client.get(ep).send()?
            }
            List => {
                let ep = format!("{url}/api/v1/timeline/list");

                client.get(ep).send()?
            }
            Prepare { id, spec } => {
                let ep = format!("{url}/api/v1/timeline/{id}/prepare");
                let file: String = std::fs::read_to_string(spec)?;

                client.post(ep).body(file).send()?
            }
            Start { env_id } => {
                let ep = format!("{url}/api/v1/env/{env_id}");

                client.post(ep).send()?
            }
            Step { env_id, num } => {
                let ep = format!("{url}/api/v1/env/{env_id}/step/{num}");

                client.post(ep).send()?
            }
            Stop { env_id } => {
                let ep = format!("{url}/api/v1/env/{env_id}");

                client.delete(ep).send()?
            }
        })
    }
}
