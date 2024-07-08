use crate::state::snarkos_status::{SnarkOSBlockInfo, SnarkOSStatus};

pub mod node;

#[tarpc::service]
pub trait AgentNodeService {
    async fn post_block_info(info: SnarkOSBlockInfo) -> Result<(), ()>;
    async fn post_status(status: SnarkOSStatus) -> Result<(), ()>;
}
