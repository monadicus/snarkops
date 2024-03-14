use serde::{Deserialize, Serialize};

pub mod infrastructure;
pub mod nodes;
pub mod outcomes;
pub mod storage;
pub mod timeline;

// TODO: Considerations:
// TODO: - Generate json schema with https://docs.rs/schemars/latest/schemars/
// TODO: - Do these types need to implement `Serialize`?

/// Deserialize (YAML) many documents into a `Vec` of documents.
pub fn deserialize_document(str: &str) -> Result<Vec<ItemDocument>, serde_yaml::Error> {
    serde_yaml::Deserializer::from_str(str)
        .map(ItemDocument::deserialize)
        .collect()
}

/// A document representing all item types.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "version")]
pub enum ItemDocument {
    #[serde(rename = "storage.snarkos.testing.monadic.us/v1")]
    Storage(storage::Document),

    #[serde(rename = "nodes.snarkos.testing.monadic.us/v1")]
    Nodes(nodes::Document),

    #[serde(rename = "infrastructure.snarkos.testing.monadic.us/v1")]
    Infrastructure(infrastructure::Document),

    #[serde(rename = "timeline.snarkos.testing.monadic.us/v1")]
    Timeline(timeline::Document),

    #[serde(rename = "outcomes.snarkos.testing.monadic.us/v1")]
    Outcomes(outcomes::Document),
}
