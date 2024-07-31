use crate::rpc::error::AgentError;

#[tarpc::service]
pub trait NodeService {
    // todo @gluax this should return an A different kind of error.
    async fn set_log_level(level: Option<String>, verbosity: Option<u8>) -> Result<(), AgentError>;
}
