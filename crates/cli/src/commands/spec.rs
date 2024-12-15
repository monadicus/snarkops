use anyhow::{anyhow, Result};
use clap::{Parser, ValueHint};
use clap_stdin::FileOrStdin;
use indexmap::IndexMap;
use reqwest::Client;
use snops_common::schema::ItemDocument;

#[derive(Debug, Parser)]
pub struct Spec {
    #[clap(subcommand)]
    pub command: SpecCommands,
}

#[derive(Debug, Parser)]
pub enum SpecCommands {
    /// Extract all node keys from a spec file.
    NodeKeys {
        /// The environment spec file.
        #[clap(value_hint = ValueHint::AnyPath)]
        spec: FileOrStdin<String>,
        /// When present, include external keys.
        #[clap(long)]
        external: bool,
    },
    /// Extract all nodes from a spec file.
    Nodes {
        /// The environment spec file.
        #[clap(value_hint = ValueHint::AnyPath)]
        spec: FileOrStdin<String>,
    },
    /// Count how many agents would be needed to run the spec.
    NumAgents {
        /// The environment spec file.
        #[clap(value_hint = ValueHint::AnyPath)]
        spec: FileOrStdin<String>,
    },
    /// Get the network id a spec.
    Network {
        /// The environment spec file.
        #[clap(value_hint = ValueHint::AnyPath)]
        spec: FileOrStdin<String>,
    },
    /// Check the spec for errors.
    Check {
        /// The environment spec file.
        #[clap(value_hint = ValueHint::AnyPath)]
        spec: FileOrStdin<String>,
    },
}

impl SpecCommands {
    pub async fn run(self, _url: &str, _client: Client) -> Result<()> {
        match self {
            SpecCommands::NodeKeys { spec, external } => {
                let docs = snops_common::schema::deserialize_docs(&spec.contents()?)?;
                let keys = docs
                    .into_iter()
                    .filter_map(|doc| doc.node_owned())
                    .flat_map(|doc| {
                        let internal = doc
                            .expand_internal_replicas()
                            .map(|r| r.0)
                            // Collection has to happen here so `doc` is dropped
                            .collect::<Vec<_>>();
                        internal.into_iter().chain(if external {
                            doc.external.into_keys().collect::<Vec<_>>()
                        } else {
                            vec![]
                        })
                    })
                    .collect::<Vec<_>>();

                println!("{}", serde_json::to_string_pretty(&keys)?);
                Ok(())
            }
            SpecCommands::Nodes { spec } => {
                let docs = snops_common::schema::deserialize_docs(&spec.contents()?)?;

                // Get nodes from the spec with retained order
                let nodes = docs
                    .into_iter()
                    .filter_map(|doc| doc.node_owned())
                    .flat_map(|doc| doc.expand_internal_replicas().collect::<IndexMap<_, _>>())
                    .collect::<IndexMap<_, _>>();

                println!("{}", serde_json::to_string_pretty(&nodes)?);
                Ok(())
            }
            SpecCommands::Network { spec } => {
                let docs = snops_common::schema::deserialize_docs(&spec.contents()?)?;
                let network = docs
                    .into_iter()
                    .filter_map(|doc| doc.node_owned())
                    .map(|doc| doc.network.unwrap_or_default())
                    .next()
                    .ok_or_else(|| anyhow!("No network id found in spec"))?;

                println!("{}", network);
                Ok(())
            }
            SpecCommands::NumAgents { spec } => {
                let docs = snops_common::schema::deserialize_docs(&spec.contents()?)?;
                let num_agents = get_num_agents_for_spec(&docs);
                println!("{num_agents}");
                Ok(())
            }
            SpecCommands::Check { spec } => {
                let _ = snops_common::schema::deserialize_docs(&spec.contents()?)?;
                println!("ok");
                Ok(())
            }
        }
    }
}

pub fn get_num_agents_for_spec(docs: &[ItemDocument]) -> usize {
    docs.iter()
        .filter_map(|doc| doc.node().map(|n| n.expand_internal_replicas().count()))
        .sum::<usize>()
}
