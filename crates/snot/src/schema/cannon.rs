use serde::Deserialize;

use crate::cannon::{sink::TxSink, source::TxSource};

/// A document describing the node infrastructure for a test.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub name: String,
    pub description: Option<String>,

    pub source: TxSource,
    pub sink: TxSink,
}
