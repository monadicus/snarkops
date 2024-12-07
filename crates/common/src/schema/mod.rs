use cannon::CannonDocument;
use error::DeserializeError;
use nodes::NodesDocument;
use serde::{Deserialize, Serialize};
use storage::StorageDocument;

use crate::state::NodeKey;

pub mod cannon;
pub mod error;
pub mod nodes;
pub mod persist;
pub mod serialize;
pub mod storage;

// TODO: Considerations:
// TODO: - Generate json schema with https://docs.rs/schemars/latest/schemars/
// TODO: - Do these types need to implement `Serialize`?

/// A document representing all item types.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "kind")]
#[non_exhaustive]
pub enum ItemDocument {
    #[serde(rename = "snops/storage/v1")]
    Storage(Box<StorageDocument>),

    #[serde(rename = "snops/nodes/v1")]
    Nodes(Box<NodesDocument>),

    #[serde(rename = "snops/cannon/v1")]
    Cannon(Box<CannonDocument>),
}

/// Deserialize (YAML) many documents into a `Vec` of documents.
pub fn deserialize_docs(str: &str) -> Result<Vec<ItemDocument>, DeserializeError> {
    serde_yaml::Deserializer::from_str(str)
        .enumerate()
        .map(|(i, doc)| ItemDocument::deserialize(doc).map_err(|e| DeserializeError { i, e }))
        .collect()
}

/// Deserialize (YAML) many documents into a `Vec` of documents.
pub fn deserialize_docs_bytes(str: &[u8]) -> Result<Vec<ItemDocument>, DeserializeError> {
    serde_yaml::Deserializer::from_slice(str)
        .enumerate()
        .map(|(i, doc)| ItemDocument::deserialize(doc).map_err(|e| DeserializeError { i, e }))
        .collect()
}

#[cfg(test)]
mod test {
    use super::deserialize_docs_bytes;

    #[test]
    fn deserialize_specs() {
        for entry in std::fs::read_dir("../../specs")
            .expect("failed to read specs dir")
            .map(Result::unwrap)
        {
            let file_name = entry.file_name();
            let name = file_name.to_str().expect("failed to read spec file name");
            if !name.ends_with(".yaml") && !name.ends_with(".yml") {
                continue;
            }

            let data = std::fs::read(entry.path()).expect("failed to read spec file");
            if let Err(e) = deserialize_docs_bytes(&data) {
                panic!("failed to deserialize spec file {name}: {e}")
            }
        }
    }
}
