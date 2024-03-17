use std::net::SocketAddr;
use std::path::PathBuf;

use snot_common::state::NodeType;

pub struct Agent {
    snarkos_process: Option<std::process::Child>,
    data_path: PathBuf,
}

impl Agent {
    pub fn get_state(&self) {}
}
