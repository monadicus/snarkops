use super::{EnvId, NodeState};

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AgentState {
    #[default]
    // A node in the inventory can function as a transaction cannon
    Inventory,
    /// Test id mapping to node state
    Node(EnvId, Box<NodeState>),
}

impl AgentState {
    pub fn map_node<F>(self, f: F) -> AgentState
    where
        F: Fn(NodeState) -> NodeState,
    {
        match self {
            Self::Inventory => Self::Inventory,
            Self::Node(id, state) => Self::Node(id, Box::new(f(*state))),
        }
    }
}
