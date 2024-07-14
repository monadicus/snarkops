use crate::state::snarkos_status::{SnarkOSBlockInfo, SnarkOSStatus};

pub mod node;

pub const PING_HEADER: &[u8] = b"snops-node";

#[tarpc::service]
pub trait AgentNodeService {
    async fn post_block_info(info: SnarkOSBlockInfo) -> Result<(), ()>;
    async fn post_status(status: SnarkOSStatus) -> Result<(), ()>;
    async fn get_log_level() -> Result<(Option<String>, Option<u8>), ()>;
}
