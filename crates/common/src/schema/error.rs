use thiserror::Error;

use crate::impl_into_status_code;

#[derive(Debug, Error)]
#[error("`{i}`: `{e}`")]
pub struct DeserializeError {
    pub i: usize,
    #[source]
    pub e: serde_yaml::Error,
}

impl_into_status_code!(DeserializeError);
