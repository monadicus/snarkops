use crate::rpc::error::AgentError;

#[tarpc::service]
pub trait NodeService {
    async fn set_log_level(level: Option<String>, verbosity: Option<u8>) -> Result<(), AgentError>;
}
