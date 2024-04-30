use snops_common::state::{CannonId, EnvId};

use super::{sink::TxSink, source::TxSource};

pub struct PersistCannon {
    pub id: CannonId,
    pub env_id: EnvId,
    pub source: TxSource,
    pub sink: TxSink,
    pub tx_count: u64,
}
