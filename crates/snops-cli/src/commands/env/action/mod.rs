use std::str::FromStr;

use anyhow::Result;
use clap::Parser;
use clap_stdin::FileOrStdin;
use reqwest::blocking::{Client, Response};
use serde_json::json;
use snops_common::{
    action_models::{AleoValue, WithTargets},
    key_source::KeySource,
    node_targets::{NodeTarget, NodeTargetError, NodeTargets},
    state::{CannonId, DocHeightRequest},
};

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
    Offline(Nodes),
    /// Turn the specified agents(and nodes) online.
    #[clap(alias = "on")]
    Online(Nodes),
    /// Reboot the specified agents(and nodes).
    Reboot(Nodes),
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
        /// The nodes to configure.
        #[clap(num_args = 1, value_delimiter = ' ')]
        nodes: Vec<NodeTarget>,
    },
}

impl Action {
    pub fn execute(self, url: &str, env_id: &str, client: Client) -> Result<Response> {
        use Action::*;
        Ok(match self {
            Offline(Nodes { nodes }) => {
                let ep = format!("{url}/api/v1/env/{env_id}/action/offline");

                client.post(ep).json(&WithTargets::from(nodes)).send()?
            }
            Online(Nodes { nodes }) => {
                let ep = format!("{url}/api/v1/env/{env_id}/action/online");

                client.post(ep).json(&WithTargets::from(nodes)).send()?
            }
            Reboot(Nodes { nodes }) => {
                let ep = format!("{url}/api/v1/env/{env_id}/action/reboot");

                client.post(ep).json(&WithTargets::from(nodes)).send()?
            }

            Execute {
                private_key,
                fee_private_key,
                cannon,
                priority_fee,
                fee_record,
                locator,
                inputs,
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

                client.post(ep).json(&json).send()?
            }
            Deploy {
                private_key,
                fee_private_key,
                cannon,
                priority_fee,
                fee_record,
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

                client.post(ep).json(&json).send()?
            }
            Config {
                online,
                height,
                peers,
                validators,
                nodes,
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

                // this api accepts a list of json objects
                client.post(ep).json(&json!(vec![json])).send()?
            }
        })
    }
}
