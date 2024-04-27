use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueHint};
use reqwest::blocking::{Client, Response};
use snops_common::state::NodeKey;

mod timeline;

/// For interacting with snop environments.
#[derive(Debug, Parser)]
pub struct Env {
    /// Show a specific env.
    #[clap(default_value = "default", value_hint = ValueHint::Other)]
    id: String,
    #[clap(subcommand)]
    command: Commands,
}

/// Env commands
#[derive(Debug, Parser)]
enum Commands {
    /// Get an env's specific agent by.
    #[clap(alias = "a")]
    Agent {
        /// The agent's key. i.e validator/0, client/foo, prover/9, compute/1,
        /// or combination.
        #[clap(value_hint = ValueHint::Other)]
        key: NodeKey,
    },

    /// List an env's agents
    Agents,

    /// Clean a specific environment.
    #[clap(alias = "c")]
    Clean,

    /// List all environments.
    /// Ignores the env id.
    #[clap(alias = "ls")]
    List,

    /// List all steps for a specific timeline.
    #[clap(alias = "t")]
    Timeline(timeline::Timeline),

    /// Show the current topology of a specific environment.
    #[clap(alias = "top")]
    Topology,

    /// Show the resolved topology of a specific environment.
    /// Shows only internal agents.
    #[clap(alias = "top-res")]
    TopologyResolved,

    /// Prepare a (test) environment.
    #[clap(alias = "p")]
    Prepare {
        /// The test spec file.
        #[clap(value_hint = ValueHint::AnyPath)]
        spec: PathBuf,
    },

    /// Get an env's storage info.
    #[clap(alias = "store")]
    Storage,

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
            Agent { key } => {
                let ep = format!("{url}/api/v1/env/{}/agents/{}", self.id, key);

                client.get(ep).send()?
            }
            Agents => {
                let ep = format!("{url}/api/v1/env/{}/agents", self.id);

                client.get(ep).send()?
            }
            Clean => {
                let ep = format!("{url}/api/v1/env/{}", self.id);

                client.delete(ep).send()?
            }
            List => {
                let ep = format!("{url}/api/v1/env/list");

                client.get(ep).send()?
            }
            Timeline(timeline) => timeline.run(url, &self.id, client)?,
            Topology => {
                let ep = format!("{url}/api/v1/env/{}/topology", self.id);

                client.get(ep).send()?
            }
            TopologyResolved => {
                let ep = format!("{url}/api/v1/env/{}/topology/resolved", self.id);

                client.get(ep).send()?
            }
            Prepare { spec } => {
                let ep = format!("{url}/api/v1/env/{}/prepare", self.id);
                let file: String = std::fs::read_to_string(spec)?;

                client.post(ep).body(file).send()?
            }
            Storage => {
                let ep = format!("{url}/api/v1/env/{}/storage", self.id);

                client.get(ep).send()?
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
