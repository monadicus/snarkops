use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueHint};
use reqwest::blocking::{Client, Response};

/// For interacting with snop environments.
#[derive(Debug, Parser)]
pub struct Env {
    /// Show a specific env.
    #[clap(default_value="default", value_hint = ValueHint::Other)]
    id: String,
    #[clap(subcommand)]
    command: Commands,
}

/// Env commands
#[derive(Debug, Parser)]
enum Commands {
    /// Clean a specific environment.
    Clean,

    /// List all steps for a specific timeline.
    Timeline {
        /// Show a specific timeline steps.
        #[clap(value_hint = ValueHint::Other)]
        timeline_id: String,
    },

    /// List all timelines for a specific environment.
    Timelines,

    /// Show the current topology of a specific environment.
    Topology,

    /// Prepare a (test) environment.
    Prepare {
        /// The test spec file.
        #[clap(value_hint = ValueHint::AnyPath)]
        spec: PathBuf,
    },

    /// Start an environment's timeline (a test).
    Start {
        /// Start a specific timeline.
        #[clap(value_hint = ValueHint::Other)]
        timeline_id: String,
    },

    /// Stop an environment's timeline.
    Stop {
        /// Stop a specific timeline.
        #[clap(value_hint = ValueHint::Other)]
        timeline_id: String,
    },
}

impl Env {
    pub fn run(self, url: &str, client: Client) -> Result<Response> {
        use Commands::*;
        Ok(match self.command {
            Clean => {
                let ep = format!("{url}/api/v1/env/{}", self.id);

                client.delete(ep).send()?
            }

            Timeline { timeline_id } => {
                let ep = format!("{url}/api/v1/env/{}/timelines/{timeline_id}/steps", self.id);

                client.get(ep).send()?
            }
            Timelines => {
                let ep = format!("{url}/api/v1/env/{}/timelines", self.id);

                client.get(ep).send()?
            }
            Topology => {
                let ep = format!("{url}/api/v1/env/{}/topology", self.id);

                client.get(ep).send()?
            }
            Prepare { spec } => {
                let ep = format!("{url}/api/v1/env/{}/prepare", self.id);
                let file: String = std::fs::read_to_string(spec)?;

                client.post(ep).body(file).send()?
            }
            Start { timeline_id } => {
                let ep = format!("{url}/api/v1/env/{}/timelines/{timeline_id}", self.id);

                client.post(ep).send()?
            }
            Stop { timeline_id } => {
                let ep = format!("{url}/api/v1/env/{}/timelines/{timeline_id}", self.id);

                client.delete(ep).send()?
            }
        })
    }
}
