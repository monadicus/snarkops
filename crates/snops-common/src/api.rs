use checkpoint::RetentionPolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StorageInfoResponse {
    pub id: String,
    pub retention_policy: Option<RetentionPolicy>,
}
