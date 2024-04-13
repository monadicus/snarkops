use std::{collections::HashMap, sync::Arc};

use snops_common::state::AgentId;

mod agent;
mod agent_flags;
pub mod document;
mod global;
mod rpc;

pub use agent::*;
pub use agent_flags::*;
pub use global::*;
pub use rpc::*;

pub type AppState = Arc<GlobalState>;
/// Map of agent ids to agents
pub type AgentPool = HashMap<AgentId, Agent>;
/// Map of agent ids to addresses for each agent.
pub type AddrMap = HashMap<AgentId, AgentAddrs>;
