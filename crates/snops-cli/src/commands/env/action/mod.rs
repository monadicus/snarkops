use anyhow::Result;
use clap::Parser;
use reqwest::blocking::{Client, Response};
use serde_json::json;
use snops_common::{
    action_models::{AleoValue, WithTargets},
    key_source::KeySource,
    node_targets::NodeTarget,
    state::CannonId,
};

//scli env canary action online client/*
//scli env canary action offline client/*

#[derive(Debug, Parser)]
pub struct Nodes {
    #[clap(num_args = 1, value_delimiter = ' ')]
    pub nodes: Vec<NodeTarget>,
}

/// For interacting with snop environments.
#[derive(Debug, Parser)]
pub enum Action {
    #[clap(alias = "off")]
    Offline(Nodes),
    #[clap(alias = "on")]
    Online(Nodes),
    Reboot(Nodes),
    // scli env canary execute credits.aleo/transfer_public committee.0 1u64
    Execute {
        /// Private key to use, can be `committee.0` to use committee member 0's
        /// key
        #[clap(long, short)]
        private_key: Option<KeySource>,
        /// Desired cannon to fire the transaction
        #[clap(long, short)]
        cannon: Option<CannonId>,
        #[clap(long, short)]
        priority_fee: Option<u32>,
        #[clap(long, short)]
        fee_record: Option<String>,
        /// `transfer_public` OR `credits.aleo/transfer_public`
        locator: String,
        /// list of program inputs
        #[clap(num_args = 1, value_delimiter = ' ')]
        inputs: Vec<AleoValue>,
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
        })
    }
}
