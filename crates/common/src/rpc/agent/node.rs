use crate::{rpc::error::AgentError, state::snarkos_status::SnarkOSLiteBlock};

#[tarpc::service]
pub trait NodeService {
    // todo @gluax this should return an A different kind of error.
    async fn status() -> Result<(), AgentError>;
    async fn set_log_level(verbosity: u8) -> Result<(), AgentError>;
    async fn get_block_lite(block_hash: String) -> Result<Option<SnarkOSLiteBlock>, AgentError>;
    async fn find_transaction(tx_id: String) -> Result<Option<String>, AgentError>;
}
