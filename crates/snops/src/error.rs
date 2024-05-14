use serde::{ser::SerializeStruct, Serialize, Serializer};
use snops_common::state::AgentId;
use snops_common::{impl_into_status_code, impl_into_type_str};
use strum_macros::AsRefStr;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("`{i}`: `{e}`")]
pub struct DeserializeError {
    pub i: usize,
    #[source]
    pub e: serde_yaml::Error,
}

impl_into_status_code!(DeserializeError);

#[derive(Debug, Error, AsRefStr)]
pub enum StateError {
    #[error(transparent)]
    Agent(#[from] snops_common::prelude::error::AgentError),
    #[error("source agent has no addr id: `{0}`")]
    NoAddress(AgentId),
    #[error(transparent)]
    Reconcile(#[from] snops_common::prelude::error::ReconcileError),
    #[error("{0}")]
    Rpc(#[from] tarpc::client::RpcError),
    #[error("source agent not found id: `{0}`")]
    SourceAgentNotFound(AgentId),
}

impl_into_status_code!(StateError);

impl_into_type_str!(StateError, |value| match value {
    Agent(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    Reconcile(e) => format!("{}.{}", value.as_ref(), e.as_ref()),
    _ => value.as_ref().to_string(),
});

impl Serialize for StateError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("type", &String::from(self))?;
        state.serialize_field("error", &self.to_string())?;

        state.end()
    }
}
