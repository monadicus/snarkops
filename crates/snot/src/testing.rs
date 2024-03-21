use bimap::BiMap;
use serde::Deserialize;
use snot_common::state::{AgentPeer, NodeKey};
use tracing::warn;

use crate::{schema::ItemDocument, state::GlobalState};

#[derive(Debug, Clone)]
pub struct Test {
    pub node_map: BiMap<NodeKey, AgentPeer>,
    // TODO: GlobalStorage.storage should maybe be here instead
}

impl Test {
    /// Deserialize (YAML) many documents into a `Vec` of documents.
    pub fn deserialize(str: &str) -> Result<Vec<ItemDocument>, serde_yaml::Error> {
        serde_yaml::Deserializer::from_str(str)
            .map(ItemDocument::deserialize)
            .collect()
    }

    /// Prepare a test. This will set the current test on the GlobalState.
    pub async fn prepare(documents: Vec<ItemDocument>, state: &GlobalState) -> anyhow::Result<()> {
        let test = Test {
            node_map: Default::default(),
        };

        for document in documents {
            match document {
                ItemDocument::Storage(storage) => storage.prepare(state).await?,
                ItemDocument::Nodes(_nodes) => {
                    // TODO: external nodes
                    // for (node_key, node) in nodes.external {
                    // }

                    // TODO: some kind of "pick_agent" function that picks an
                    // agent best suited to be a node,
                    // instead of naively picking an agent to fill the needs of
                    // a node

                    // TODO: internal nodes
                    // TODO: populate test.node_map after delegating agents to
                    // become test nodes
                }

                _ => warn!("ignored unimplemented document type"),
            }
        }

        // set the test on the global state
        *state.test.write().await = Some(test);

        Ok(())
    }
}
