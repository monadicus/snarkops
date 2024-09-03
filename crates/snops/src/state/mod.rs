use std::{collections::HashMap, sync::Arc};

use dashmap::DashMap;
use snops_common::state::{AgentId, EnvId, NetworkId, StorageId};

mod agent;
mod agent_flags;
pub mod error;
pub mod external_peers;
mod global;
mod reconcile;
mod rpc;
pub mod snarkos_request;
pub mod transactions;

pub use agent::*;
pub use agent_flags::*;
pub use global::*;
pub use reconcile::*;
pub use rpc::*;

use crate::{env::Environment, schema::storage::LoadedStorage};

pub type AppState = Arc<GlobalState>;
/// Map of agent ids to agents
pub type AgentPool = DashMap<AgentId, Agent>;
/// Map of storage ids to storage info
pub type StorageMap = DashMap<(NetworkId, StorageId), Arc<LoadedStorage>>;
/// Map of environment ids to environments
pub type EnvMap = DashMap<EnvId, Arc<Environment>>;
/// Map of agent ids to addresses for each agent.
pub type AddrMap = HashMap<AgentId, AgentAddrs>;
