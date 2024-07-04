use std::str::FromStr;

use anyhow::Result;
use clap::{error::ErrorKind, ArgGroup, CommandFactory, Parser, ValueHint};
use reqwest::blocking::{Client, Response};
use serde_json::json;
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

/// Agent commands.
#[derive(Debug, Parser)]
enum AgentCommands {
    /// Find agents by set criteria.
    /// If all of client/compute/prover/validator are not specified it can be
    /// any one of them.
    #[clap(group(ArgGroup::new("environment").required(false).args(&["env", "all"])))]
    Find {
        /// Whether the agent can be a client.
        #[clap(long)]
        client: bool,
        /// Whether the agent can be a compute.
        #[clap(long)]
        compute: bool,
        /// Whether the agent can be a prover.
        #[clap(long)]
        prover: bool,
        /// Whether the agent can be a validator.
        #[clap(long)]
        validator: bool,
        /// Which env you are finding the agens from.
        /// Not specifing a env, means only inventoried agents are found.
        #[clap(long, group = "environment")]
        env: Option<String>,
        /// Means regardless of connection status, and state we find them.
        #[clap(long, group = "environment")]
        all: bool,
        /// The labels an agent should have.
        #[clap(long, value_delimiter = ',', num_args = 1..)]
        labels: Vec<String>,
        /// If the agent has a local private key or not.
        #[clap(long)]
        local_pk: bool,
        /// Wether to include offline agents as well.
        #[clap(long)]
        include_offline: bool,
    },
    /// Get the specific agent.
    #[clap(alias = "i")]
    Info,
    /// Kill the specific agent
    Kill,

    /// List all agents.
    /// Ignores the agent id.
    #[clap(alias = "ls")]
    List,

    /// Get the specific agent's TPS.
    Tps,

    SetLogLevel {
        /// The log level to set.
        level: String,
    },
}

impl Agent {
    pub fn run(self, url: &str, client: Client) -> Result<Response> {
        use AgentCommands::*;
        Ok(match self.command {
            Find {
                env,
                labels,
                all,
                include_offline,
                local_pk,
                client: mode_client,
                compute,
                prover,
                validator,
            } => {
                let ep = format!("{url}/api/v1/agents/find");

                client
                    .post(ep)
                    .json(&json!({
                        "mode": {
                          "client": mode_client,
                          "compute": compute,
                          "prover": prover,
                          "validator": validator,
                        },
                        "env": env,
                        "labels": labels,
                        "all": all,
                        "include_offline": include_offline,
                        "local_pk": local_pk,
                    }))
                    .send()?
            }
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
            Kill => {
                let ep = format!("{url}/api/v1/agents/{}/kill", self.id);

                client.post(ep).send()?
            }
            Tps => {
                let ep = format!("{url}/api/v1/agents/{}/tps", self.id);

                client.get(ep).send()?
            }
            SetLogLevel { level } => {
                let ep = format!("{url}/api/v1/agents/{}/log/{}", self.id, level);

                client.post(ep).send()?
            }
        })
    }
}
