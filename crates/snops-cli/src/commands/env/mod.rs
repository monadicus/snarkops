use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueHint};
use clap_stdin::FileOrStdin;
use reqwest::blocking::{Client, Response};
use snops_common::{
    action_models::AleoValue,
    aot_cmds::Authorization,
    key_source::KeySource,
    state::{CannonId, InternedId, NodeKey},
};

mod action;

/// For interacting with snop environments.
#[derive(Debug, Parser)]
pub struct Env {
    /// Work with a specific env.
    #[clap(default_value = "default", value_hint = ValueHint::Other)]
    id: InternedId,
    #[clap(subcommand)]
    command: EnvCommands,
}

/// Env commands.
#[derive(Debug, Parser)]
enum EnvCommands {
    #[clap(subcommand)]
    Action(action::Action),
    /// Get an env's specific agent by.
    #[clap(alias = "a")]
    Agent {
        /// The agent's key. i.e validator/0, client/foo, prover/9,
        /// or combination.
        #[clap(value_hint = ValueHint::Other)]
        key: NodeKey,
    },

    /// List an env's agents
    Agents,
    // execute and broadcast
    Auth {
        /// When present, don't wait for transaction execution before returning
        #[clap(long = "async")]
        async_mode: bool,
        /// Desired cannon to fire the transaction
        #[clap(long, short, default_value = "default")]
        cannon: CannonId,
        /// Authorization to execute and broadcast
        auth: FileOrStdin<Authorization>,
    },

    /// Lookup an account's balance
    #[clap(alias = "bal")]
    Balance {
        /// Address to lookup balance for
        address: KeySource,
    },

    /// Lookup a block or get the latest block
    Block {
        /// The block's height or hash.
        #[clap(default_value = "latest")]
        height_or_hash: String,
    },

    /// Get the latest height from all agents in the env.
    Height,

    /// Lookup a transaction's block by a transaction id.
    #[clap(alias = "tx")]
    Transaction { id: String },

    /// Lookup a transaction's details by a transaction id.
    #[clap(alias = "tx-details")]
    TransactionDetails { id: String },

    /// Clean a specific environment.
    #[clap(alias = "c")]
    Clean,

    /// Get an env's latest block/state root info.
    Info,

    /// List all environments.
    /// Ignores the env id.
    #[clap(alias = "ls")]
    List,

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

    /// Lookup a mapping by program id and mapping name.
    Mapping {
        /// The program name.
        program: String,
        /// The mapping name.
        mapping: String,
        /// The key to lookup.
        key: AleoValue,
    },
    /// Lookup a program's mappings only.
    Mappings {
        /// The program name.
        program: String,
    },
    /// Lookup a program by its id.
    Program { id: String },

    /// Get an env's snarkos network.
    Network,

    /// Get an env's storage info.
    #[clap(alias = "store")]
    Storage,
}

impl Env {
    pub fn run(self, url: &str, client: Client) -> Result<Response> {
        let id = self.id;
        use EnvCommands::*;
        Ok(match self.command {
            Action(action) => action.execute(url, id, client)?,
            Agent { key } => {
                let ep = format!("{url}/api/v1/env/{id}/agents/{key}");

                client.get(ep).send()?
            }
            Agents => {
                let ep = format!("{url}/api/v1/env/{id}/agents");

                client.get(ep).send()?
            }
            Auth {
                async_mode,
                cannon,
                auth,
            } => {
                let ep = format!("{url}/api/v1/env/{id}/cannons/{cannon}/auth");

                let mut req = client.post(ep).json(&auth.contents()?);

                if async_mode {
                    req = req.query(&[("async", "true")]);
                }

                req.send()?
            }
            Balance { address: key } => {
                let ep = format!("{url}/api/v1/env/{id}/balance/{key}");

                client.get(ep).json(&key).send()?
            }
            Block { height_or_hash } => {
                let ep = format!("{url}/api/v1/env/{id}/block/{height_or_hash}");

                client.get(ep).send()?
            }
            Clean => {
                let ep = format!("{url}/api/v1/env/{id}");

                client.delete(ep).send()?
            }
            Info => {
                let ep = format!("{url}/api/v1/env/{id}/info");

                client.get(ep).send()?
            }
            List => {
                let ep = format!("{url}/api/v1/env/list");

                client.get(ep).send()?
            }
            Topology => {
                let ep = format!("{url}/api/v1/env/{id}/topology");

                client.get(ep).send()?
            }
            TopologyResolved => {
                let ep = format!("{url}/api/v1/env/{id}/topology/resolved");

                client.get(ep).send()?
            }
            Prepare { spec } => {
                let ep = format!("{url}/api/v1/env/{id}/prepare");
                let file: String = std::fs::read_to_string(spec)?;

                client.post(ep).body(file).send()?
            }
            Mapping {
                program,
                mapping,
                key,
            } => {
                let ep = match key {
                    AleoValue::Other(key) => {
                        format!(
                            "{url}/api/v1/env/{id}/program/{program}/mapping/{mapping}?key={key}"
                        )
                    }
                    AleoValue::Key(source) => {
                        format!(
                            "{url}/api/v1/env/{id}/program/{program}/mapping/{mapping}?keysource={source}"
                        )
                    }
                };

                client.get(ep).send()?
            }
            Mappings { program } => {
                let ep = format!("{url}/api/v1/env/{id}/program/{program}/mappings");

                client.get(ep).send()?
            }
            Program { id: prog } => {
                let ep = format!("{url}/api/v1/env/{id}/program/{prog}");

                println!("{}", client.get(ep).send()?.text()?);
                std::process::exit(0);
            }
            Network => {
                let ep = format!("{url}/api/v1/env/{id}/network");

                client.get(ep).send()?
            }
            Storage => {
                let ep = format!("{url}/api/v1/env/{id}/storage");

                client.get(ep).send()?
            }
            Transaction { id: hash } => {
                let ep = format!("{url}/api/v1/env/{id}/transaction_block/{hash}");

                client.get(ep).send()?
            }
            TransactionDetails { id: hash } => {
                let ep = format!("{url}/api/v1/env/{id}/transaction/{hash}");

                client.get(ep).send()?
            }
            Height => {
                let ep = format!("{url}/api/v1/env/{id}/height");

                client.get(ep).send()?
            }
        })
    }
}
