use std::net::IpAddr;

use snops_common::state::{AgentState, InternedId};

use crate::state::Agent;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentStatusResponse {
    pub agent_id: InternedId,
    pub is_connected: bool,
    pub is_computing: bool,
    pub external_ip: Option<IpAddr>,
    pub internal_ip: Option<IpAddr>,
    pub state: AgentState,
}

impl From<&Agent> for AgentStatusResponse {
    fn from(agent: &Agent) -> Self {
        Self {
            agent_id: agent.id(),
            is_connected: agent.is_connected(),
            is_computing: agent.is_compute_claimed(),
            external_ip: agent.addrs().and_then(|a| a.external),
            internal_ip: agent.addrs().and_then(|a| a.internal.first().cloned()),
            state: agent.state().clone(),
        }
    }
}
