use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// A document describing a test's expected outcomes.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Document {
    pub metrics: Metrics,
}

// TODO: this definitely needs to be a lot more specific...
pub type Metrics = IndexMap<String, serde_yaml::Value>;
