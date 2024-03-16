use std::net::SocketAddr;
use std::path::PathBuf;

use snot_common::state::NodeType;

pub struct Agent {
    snarkos_process: Option<std::process::Child>,
    data_path: PathBuf,
}

enum StateUpdate {
    Online(bool),
    NodeType(NodeType),
    Genesis(String),
    SnarkosPeers(Vec<SocketAddr>),
    SnarkosValidators(Vec<SocketAddr>),
    Block { height: u32, timestamp: i64 },
}

impl Agent {
    pub fn get_state(&self) {}
}
