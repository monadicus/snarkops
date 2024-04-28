use std::str::FromStr;

use anyhow::Result;
use clap::{error::ErrorKind, CommandFactory, Parser, ValueHint};
use reqwest::blocking::{Client, Response};
use snops_common::state::TimelineId;

use crate::{cli::Cli, commands::DUMMY_ID};

/// For interacting with snop environments.
#[derive(Debug, Parser)]
pub struct Timeline {
    /// The timeline id.
    #[clap(value_hint = ValueHint::Other, default_value = DUMMY_ID)]
    id: TimelineId,
    #[clap(subcommand)]
    command: Commands,
}

/// Env commands
#[derive(Debug, Parser)]
enum Commands {
    /// Apply a timeline to an environment.
    #[clap(alias = "a")]
    Apply,

    /// Delete a timeline from an environment.zs
    #[clap(alias = "d")]
    Delete,

    /// List all steps for a specific timeline.
    #[clap(alias = "i")]
    Info,

    /// List all timelines for a specific environment.
    /// Timeline id is ignored.
    #[clap(alias = "ls")]
    List,
}

impl Timeline {
    pub fn run(self, url: &str, env_id: &str, client: Client) -> Result<Response> {
        use Commands::*;
        Ok(match self.command {
            List => {
                let ep = format!("{url}/api/v1/env/{env_id}/timelines");

                client.get(ep).send()?
            }
            _ if self.id == TimelineId::from_str(DUMMY_ID).unwrap() => {
                let mut cmd = Cli::command();
                cmd.error(
                    ErrorKind::MissingRequiredArgument,
                    " the following required arguments were not provided:\n  <ID>",
                )
                .exit();
            }
            Apply => {
                let ep = format!("{url}/api/v1/env/{env_id}/timelines/{}", self.id);

                client.post(ep).send()?
            }
            Delete => {
                let ep = format!("{url}/api/v1/env/{env_id}/timelines/{}", self.id);

                client.delete(ep).send()?
            }
            Info => {
                let ep = format!("{url}/api/v1/env/{env_id}/timelines/{}/steps", self.id);

                client.get(ep).send()?
            }
        })
    }
}
