use serde::{Deserialize, Serialize};
use sink::TxSink;
use source::TxSource;

pub mod sink;
pub mod source;
use crate::state::CannonId;

/// A document describing the node infrastructure for a test.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CannonDocument {
    pub name: CannonId,
    pub description: Option<String>,

    pub source: TxSource,
    pub sink: TxSink,
}
