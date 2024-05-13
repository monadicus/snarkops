use std::fmt::Display;

#[derive(
    Default, Clone, Copy, Debug, serde::Serialize, serde::Deserialize, clap::Parser, PartialEq, Eq,
)]
pub struct AgentMode {
    /// Enable running a validator node
    #[arg(long)]
    pub validator: bool,

    /// Enable running a prover node
    #[arg(long)]
    pub prover: bool,

    /// Enable running a client node
    #[arg(long)]
    pub client: bool,

    /// Enable functioning as a compute target when inventoried
    #[arg(long)]
    pub compute: bool,
}

impl From<AgentMode> for u8 {
    fn from(mode: AgentMode) -> u8 {
        (mode.validator as u8)
            | (mode.prover as u8) << 1
            | (mode.client as u8) << 2
            | (mode.compute as u8) << 3
    }
}

impl From<u8> for AgentMode {
    fn from(mode: u8) -> Self {
        Self {
            validator: mode & 1 != 0,
            prover: mode & 1 << 1 != 0,
            client: mode & 1 << 2 != 0,
            compute: mode & 1 << 3 != 0,
        }
    }
}

impl Display for AgentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        if self.validator {
            s.push_str("validator");
        }
        if self.prover {
            if !s.is_empty() {
                s.push_str(", ");
            }
            s.push_str("prover");
        }
        if self.client {
            if !s.is_empty() {
                s.push_str(", ");
            }
            s.push_str("client");
        }
        if self.compute {
            if !s.is_empty() {
                s.push_str(", ");
            }
            s.push_str("compute");
        }

        f.write_str(&s)
    }
}
