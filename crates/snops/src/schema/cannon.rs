use serde::Deserialize;
use snops_common::state::CannonId;

use crate::cannon::{sink::TxSink, source::TxSource};

/// A document describing the node infrastructure for a test.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub name: CannonId,
    pub description: Option<String>,

    pub source: TxSource,
    pub sink: TxSink,
    #[serde(default)]
    /// When true, create an instance of the cannon when the document is loaded
    pub instance: bool,
    /// Number of transactions to fire when for an instanced cannon is created
    #[serde(default)]
    pub count: Option<usize>,
}
