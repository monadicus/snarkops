use hmac::{Hmac, Mac};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use snops_common::state::AgentId;

lazy_static! {
    pub static ref JWT_SECRET: Hmac<Sha256> = Hmac::new_from_slice(
        std::env::var("JWT_SECRET")
            .unwrap_or("secret".to_string())
            .as_bytes()
    )
    .unwrap();
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub id: AgentId,
    pub nonce: u16,
}
