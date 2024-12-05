use std::collections::HashMap;

use action::post_and_wait_tx;
use anyhow::Result;
use clap::{Parser, ValueHint};
use clap_stdin::FileOrStdin;
use reqwest::blocking::{Client, RequestBuilder, Response};
use snops_cli::events::EventsClient;
use snops_common::{
    action_models::AleoValue,
    events::{AgentEvent, Event, EventKind},
    key_source::KeySource,
    state::{AgentId, Authorization, CannonId, EnvId, InternedId, NodeKey, ReconcileStatus},
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

    /// Delete a specific environment.
    #[clap(alias = "d")]
    Delete,

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

    /// Apply an environment spec.
    #[clap(alias = "p")]
    Apply {
        /// The environment spec file.
        #[clap(value_hint = ValueHint::AnyPath)]
        spec: FileOrStdin<String>,
        /// When present, don't wait for reconciles to finish before returning
        #[clap(long = "async")]
        async_mode: bool,
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

    /// Get an env's storage info.
    #[clap(alias = "store")]
    Storage,
}

impl Env {
    pub async fn run(self, url: &str, client: Client) -> Result<Response> {
        let id = self.id;
        use EnvCommands::*;
        Ok(match self.command {
            Action(action) => action.execute(url, id, client).await?,
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

                if async_mode {
                    req.send()?
                } else {
                    post_and_wait_tx(url, req).await?;
                    std::process::exit(0);
                }
            }
            Balance { address: key } => {
                let ep = format!("{url}/api/v1/env/{id}/balance/{key}");

                client.get(ep).json(&key).send()?
            }
            Block { height_or_hash } => {
                let ep = format!("{url}/api/v1/env/{id}/block/{height_or_hash}");

                client.get(ep).send()?
            }
            Delete => {
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
            Apply { spec, async_mode } => {
                let ep = format!("{url}/api/v1/env/{id}/apply");
                let req = client.post(ep).body(spec.contents()?);
                if async_mode {
                    req.send()?
                } else {
                    post_and_wait(url, req, id).await?;
                    std::process::exit(0);
                }
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

pub async fn post_and_wait(url: &str, req: RequestBuilder, env_id: EnvId) -> Result<()> {
    use snops_common::events::EventFilter::*;
    use snops_common::events::EventKindFilter::*;

    let mut events = EventsClient::open_with_filter(
        url,
        EnvIs(env_id)
            & (AgentConnected
                | AgentDisconnected
                | AgentReconcile
                | AgentReconcileComplete
                | AgentReconcileError),
    )
    .await?;

    let mut node_map: HashMap<NodeKey, AgentId> = req.send()?.json()?;
    println!("{}", serde_json::to_string_pretty(&node_map)?);

    let filter = node_map
        .values()
        .copied()
        .fold(!Unfiltered, |id, filter| (id | AgentIs(filter)));

    while let Some(event) = events.next().await? {
        // Ensure the event is based on the response
        if !event.matches(&filter) {
            continue;
        }

        if let Event {
            node_key: Some(node),
            content: EventKind::Agent(e),
            ..
        } = &event
        {
            match &e {
                AgentEvent::Reconcile(ReconcileStatus {
                    scopes, conditions, ..
                }) => {
                    println!(
                        "{node}: {} {}",
                        scopes.join(";"),
                        conditions
                            .iter()
                            // unwrap safety - it was literally just serialized
                            .map(|s| serde_json::to_string(s).unwrap())
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                }
                AgentEvent::ReconcileError(err) => {
                    println!("{node}: error: {err}");
                }
                AgentEvent::ReconcileComplete => {
                    println!("{node}: done");
                }
                _ => {}
            }
        }
        if let (Some(node_key), true) = (
            event.node_key.as_ref(),
            event.matches(&AgentReconcileComplete.into()),
        ) {
            node_map.remove(node_key);
            if node_map.is_empty() {
                break;
            }
        }
    }
    events.close().await
}
