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
}
