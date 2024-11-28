use lazy_static::lazy_static;
use regex::Regex;

mod agent_mode;
mod agent_state;
mod agent_status;
mod height_request;
mod id;
mod network;
mod node_key;
mod node_state;
mod node_type;
mod port_config;
mod reconcile;
pub mod snarkos_status;
pub mod strings;

pub use agent_mode::*;
pub use agent_state::*;
pub use agent_status::*;
pub use height_request::*;
pub use id::*;
pub use network::*;
pub use node_key::*;
pub use node_state::*;
pub use node_type::*;
pub use port_config::*;
pub use reconcile::*;

lazy_static! {
    static ref NODE_KEY_REGEX: Regex = Regex::new(
        r"^(?P<ty>client|validator|prover)\/(?P<id>[A-Za-z0-9\-]*)(?:@(?P<ns>[A-Za-z0-9\-]+))?$"
    )
    .unwrap();
    static ref INTERNED_ID_REGEX: Regex =
        Regex::new(r"^[A-Za-z0-9][A-Za-z0-9\-_.]{0,63}$").unwrap();
}

pub type AgentId = InternedId;
pub type EnvId = InternedId;
pub type CannonId = InternedId;
pub type StorageId = InternedId;
pub type TimelineId = InternedId;
pub type TxPipeId = InternedId;
