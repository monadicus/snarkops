use std::{collections::HashMap, sync::Arc};

use snops_common::state::{AgentId, EnvId, StorageId};

mod agent;
mod agent_flags;
mod global;
pub mod persist;
mod rpc;

pub use agent::*;
pub use agent_flags::*;
pub use global::*;
pub use rpc::*;

use crate::{env::Environment, schema::storage::LoadedStorage};

pub type AppState = Arc<GlobalState>;
/// Map of agent ids to agents
pub type AgentPool = HashMap<AgentId, Agent>;
/// Map of storage ids to storage info
pub type StorageMap = HashMap<StorageId, Arc<LoadedStorage>>;
/// Map of environment ids to environments
pub type EnvMap = HashMap<EnvId, Arc<Environment>>;
/// Map of agent ids to addresses for each agent.
pub type AddrMap = HashMap<AgentId, AgentAddrs>;
