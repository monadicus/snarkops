use serde::Deserialize;
use snops_common::state::NodeKey;

pub mod cannon;
pub mod error;
pub mod infrastructure;
pub mod nodes;
pub mod outcomes;
pub mod storage;

// TODO: Considerations:
// TODO: - Generate json schema with https://docs.rs/schemars/latest/schemars/
// TODO: - Do these types need to implement `Serialize`?

/// A document representing all item types.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "version")]
#[non_exhaustive]
pub enum ItemDocument {
    #[serde(rename = "storage.snarkos.testing.monadic.us/v1")]
    Storage(Box<storage::Document>),

    #[serde(rename = "nodes.snarkos.testing.monadic.us/v1")]
    Nodes(Box<nodes::Document>),

    #[serde(rename = "infrastructure.snarkos.testing.monadic.us/v1")]
    Infrastructure(Box<infrastructure::Document>),

    #[serde(rename = "cannon.snarkos.testing.monadic.us/v1")]
    Cannon(Box<cannon::Document>),
}

#[cfg(test)]
mod test {
    use crate::env::Environment;

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
            if let Err(e) = Environment::deserialize_bytes(&data) {
                panic!("failed to deserialize spec file {name}: {e}")
            }
        }
    }
}
