use std::{collections::HashMap, str::FromStr};

use anyhow::Result;
use clap::Parser;
use clap_stdin::FileOrStdin;
use reqwest::blocking::{Client, Response};
use serde_json::json;
use snops_common::{
    action_models::{AleoValue, WithTargets},
    key_source::KeySource,
    node_targets::{NodeTarget, NodeTargetError, NodeTargets},
    state::{CannonId, DocHeightRequest, EnvId, InternedId},
};

use crate::commands::env::post_and_wait;

//scli env canary action online client/*
//scli env canary action offline client/*

#[derive(Clone, Debug, Parser)]
pub struct Nodes {
    #[clap(num_args = 1, value_delimiter = ' ')]
    pub nodes: Vec<NodeTarget>,
}

#[derive(Clone, Debug)]
pub enum NodesOption {
    None,
    Nodes(NodeTargets),
}

impl FromStr for NodesOption {
    type Err = NodeTargetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "none" {
            Ok(NodesOption::None)
        } else {
            Ok(NodesOption::Nodes(
                s.split(',')
                    .map(NodeTarget::from_str)
                    .collect::<Result<Vec<_>, Self::Err>>()?
                    .into(),
            ))
        }
    }
}

impl From<NodesOption> for NodeTargets {
    fn from(opt: NodesOption) -> Self {
        match opt {
            NodesOption::None => NodeTargets::None,
            NodesOption::Nodes(nodes) => nodes,
        }
    }
}

/// Actions you can apply on a specific environment.
#[derive(Debug, Parser)]
pub enum Action {
    /// Turn the specified agents(and nodes) offline.
    #[clap(alias = "off")]
    Offline {
        #[clap(num_args = 1, value_delimiter = ' ')]
        nodes: Vec<NodeTarget>,
        /// When present, don't wait for reconciles to finish before returning
        #[clap(long = "async")]
        async_mode: bool,
    },
    /// Turn the specified agents(and nodes) online.
    #[clap(alias = "on")]
    Online {
        #[clap(num_args = 1, value_delimiter = ' ')]
        nodes: Vec<NodeTarget>,
        /// When present, don't wait for reconciles to finish before returning
        #[clap(long = "async")]
        async_mode: bool,
    },
    /// Reboot the specified agents(and nodes).
    Reboot {
        #[clap(num_args = 1, value_delimiter = ' ')]
        nodes: Vec<NodeTarget>,
        /// When present, don't wait for reconciles to finish before returning
        #[clap(long = "async")]
        async_mode: bool,
    },
    /// Execute an aleo program function on the environment. i.e.
    /// credits.aleo/transfer_public
    Execute {
        /// Private key to use, can be `committee.0` to use committee member 0's
        /// key
        #[clap(long, short)]
        private_key: Option<KeySource>,
        /// Private key to use for the fee. Defaults to the same as
        /// --private-key
        #[clap(long)]
        fee_private_key: Option<KeySource>,
        /// Desired cannon to fire the transaction
        #[clap(long, short)]
        cannon: Option<CannonId>,
        /// The optional priority fee to use.
        #[clap(long)]
        priority_fee: Option<u32>,
        /// The fee record to use if you want to pay the fee privately.
        #[clap(long)]
        fee_record: Option<String>,
        /// When present, don't wait for transaction execution before returning
        #[clap(long = "async")]
        async_mode: bool,
        /// `transfer_public` OR `credits.aleo/transfer_public`.
        locator: String,
        /// list of program inputs.
        #[clap(num_args = 1, value_delimiter = ' ')]
        inputs: Vec<AleoValue>,
    },
    /// Deploy an aleo program to the environment.
    Deploy {
        /// Private key to use, can be `committee.0` to use committee member 0's
        /// key
        #[clap(long, short)]
        private_key: Option<KeySource>,
        /// Private key to use for the fee. Defaults to the same as
        /// --private-key
        #[clap(long)]
        fee_private_key: Option<KeySource>,
        /// Desired cannon to fire the transaction
        #[clap(long, short)]
        cannon: Option<CannonId>,
        /// The optional priority fee to use.
        #[clap(long)]
        priority_fee: Option<u32>,
        /// The fee record to use if you want to pay the fee privately.
        #[clap(long)]
        fee_record: Option<String>,
        /// When present, don't wait for transaction execution before returning
        #[clap(long = "async")]
        async_mode: bool,
        /// Path to program or program content in stdin
        program: FileOrStdin<String>,
    },
    /// Configure the state of the target nodes.
    Config {
        /// Configure the online state of the target nodes.
        #[clap(long, short)]
        online: Option<bool>,
        /// Configure the height of the target nodes.
        #[clap(long)]
        height: Option<DocHeightRequest>,
        /// Configure the peers of the target nodes, or `none`.
        #[clap(long, short)]
        peers: Option<NodesOption>,
        /// Configure the validators of the target nodes, or `none`.
        #[clap(long, short)]
        validators: Option<NodesOption>,
        /// Set environment variables for a node: `--env FOO=bar`
        #[clap(long, short, number_of_values = 1, value_parser = clap::value_parser!(KeyEqValue))]
        env: Option<Vec<KeyEqValue>>,
        // Remove environment variables from a node: `--del-env FOO,BAR`
        #[clap(long, short, value_delimiter = ',', allow_hyphen_values = true)]
        del_env: Option<Vec<String>>,
        /// The nodes to configure.
        #[clap(num_args = 1, value_delimiter = ' ')]
        nodes: Vec<NodeTarget>,
        /// Configure the binary for a node.
        #[clap(long, short)]
        binary: Option<InternedId>,
        /// Configure the private key for a node.
        #[clap(long, short)]
        private_key: Option<KeySource>,
        #[clap(long = "async")]
        async_mode: bool,
    },
}

#[derive(Clone, Debug)]
pub struct KeyEqValue(pub String, pub String);

impl FromStr for KeyEqValue {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let (l, r) = s.split_once('=').ok_or("missing = in `key=value`")?;
        Ok(Self(l.to_owned(), r.to_owned()))
    }
}

impl KeyEqValue {
    pub fn pair(self) -> (String, String) {
        (self.0, self.1)
    }
}

impl Action {
    pub async fn execute(self, url: &str, env_id: EnvId, client: Client) -> Result<Response> {
        use Action::*;
        Ok(match self {
            Offline { nodes, async_mode } => {
                let ep = format!("{url}/api/v1/env/{env_id}/action/offline");
                let req = client.post(ep).json(&WithTargets::from(nodes));
                if async_mode {
                    req.send()?
                } else {
                    post_and_wait(url, req, env_id).await?;
                    std::process::exit(0);
                }
            }
            Online { nodes, async_mode } => {
                let ep = format!("{url}/api/v1/env/{env_id}/action/online");
                let req = client.post(ep).json(&WithTargets::from(nodes));
                if async_mode {
                    req.send()?
                } else {
                    post_and_wait(url, req, env_id).await?;
                    std::process::exit(0);
                }
            }
            Reboot { nodes, async_mode } => {
                let ep = format!("{url}/api/v1/env/{env_id}/action/reboot");
                let req = client.post(ep).json(&WithTargets::from(nodes));
                if async_mode {
                    req.send()?
                } else {
                    post_and_wait(url, req, env_id).await?;
                    std::process::exit(0);
                }
            }

            Execute {
                private_key,
                fee_private_key,
                cannon,
                priority_fee,
                fee_record,
                locator,
                inputs,
                async_mode,
            } => {
                let ep = format!("{url}/api/v1/env/{}/action/execute", env_id);

                let (program, function) = locator
                    .split_once('/')
                    .map(|(program, function)| (Some(program), function))
                    .unwrap_or((None, &locator));

                let mut json = json!({
                    "function": function,
                    "inputs": inputs,
                });

                if let Some(private_key) = private_key {
                    json["private_key"] = private_key.to_string().into();
                }
                if let Some(fee_private_key) = fee_private_key {
                    json["fee_private_key"] = fee_private_key.to_string().into();
                }
                if let Some(cannon) = cannon {
                    json["cannon"] = cannon.to_string().into();
                }
                if let Some(priority_fee) = priority_fee {
                    json["priority_fee"] = priority_fee.into();
                }
                if let Some(fee_record) = fee_record {
                    json["fee_record"] = fee_record.into();
                }

                if let Some(program) = program {
                    json["program"] = program.into();
                }

                let mut builder = client.post(ep);
                if async_mode {
                    let query = [("async", "true")];
                    builder = builder.query(&query);
                }
                builder.json(&json).send()?
            }
            Deploy {
                private_key,
                fee_private_key,
                cannon,
                priority_fee,
                fee_record,
                async_mode,
                program,
            } => {
                let ep = format!("{url}/api/v1/env/{}/action/deploy", env_id);

                let mut json = json!({
                    "program": program.contents()?,
                });

                if let Some(private_key) = private_key {
                    json["private_key"] = private_key.to_string().into();
                }
                if let Some(fee_private_key) = fee_private_key {
                    json["fee_private_key"] = fee_private_key.to_string().into();
                }
                if let Some(cannon) = cannon {
                    json["cannon"] = cannon.to_string().into();
                }
                if let Some(priority_fee) = priority_fee {
                    json["priority_fee"] = priority_fee.into();
                }
                if let Some(fee_record) = fee_record {
                    json["fee_record"] = fee_record.into();
                }

                let mut builder = client.post(ep);
                if async_mode {
                    let query = [("async", "true")];
                    builder = builder.query(&query);
                }
                builder.json(&json).send()?
            }
            Config {
                online,
                height,
                peers,
                validators,
                nodes,
                env,
                del_env,
                binary,
                private_key,
                async_mode,
            } => {
                let ep = format!("{url}/api/v1/env/{env_id}/action/config");

                let mut json = json!({
                    "nodes": NodeTargets::from(nodes),
                });

                if let Some(online) = online {
                    json["online"] = online.into();
                }
                if let Some(height) = height {
                    json["height"] = json!(height);
                }
                if let Some(peers) = peers {
                    json["peers"] = json!(NodeTargets::from(peers));
                }
                if let Some(validators) = validators {
                    json["validators"] = json!(NodeTargets::from(validators));
                }
                if let Some(binary) = binary {
                    json["binary"] = json!(binary);
                }
                if let Some(private_key) = private_key {
                    json["private_key"] = json!(private_key);
                }
                if let Some(env) = env {
                    json["set_env"] = json!(env
                        .into_iter()
                        .map(KeyEqValue::pair)
                        .collect::<HashMap<_, _>>())
                }
                if let Some(del_env) = del_env {
                    json["del_env"] = json!(del_env)
                }

                // this api accepts a list of json objects
                let req = client.post(ep).json(&json!(vec![json]));

                if async_mode {
                    req.send()?
                } else {
                    post_and_wait(url, req, env_id).await?;
                    std::process::exit(0);
                }
            }
        })
    }
}
