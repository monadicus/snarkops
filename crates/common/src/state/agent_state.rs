use super::{EnvId, NodeState};
use crate::format::{DataFormat, DataHeaderOf};

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

impl DataFormat for AgentState {
    type Header = (u8, DataHeaderOf<NodeState>);
    const LATEST_HEADER: Self::Header = (1, NodeState::LATEST_HEADER);

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        match self {
            Self::Inventory => Ok(0u8.write_data(writer)?),
            Self::Node(id, state) => {
                let mut written = 1u8.write_data(writer)?;
                written += id.write_data(writer)?;
                written += state.write_data(writer)?;
                Ok(written)
            }
        }
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if header.0 != Self::LATEST_HEADER.0 {
            return Err(crate::format::DataReadError::unsupported(
                "AgentState",
                Self::LATEST_HEADER.0,
                header.0,
            ));
        }

        match u8::read_data(reader, &())? {
            0 => Ok(Self::Inventory),
            1 => {
                let id = EnvId::read_data(reader, &())?;
                let state = NodeState::read_data(reader, &header.1)?;
                Ok(Self::Node(id, Box::new(state)))
            }
            n => Err(crate::format::DataReadError::custom(format!(
                "Invalid AgentState variant {n}",
            ))),
        }
    }
}
