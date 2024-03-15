use indexmap::IndexMap;
use serde::Deserialize;

/// A document describing a test's expected outcomes.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub metrics: Metrics,
}

// TODO: this definitely needs to be a lot more specific...
pub type Metrics = IndexMap<String, serde_yaml::Value>;
