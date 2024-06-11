use std::thread;

use reqwest::blocking::Client;
use snarkvm::{
    ledger::store::{helpers::rocksdb::BlockDB, BlockStorage},
    prelude::Network,
};
use snops_common::state::snarkos_status::{SnarkOSBlockInfo, SnarkOSStatus};

pub enum AgentStatusClient {
    Enabled { client: Client, port: u16 },
    Disabled,
}

impl From<Option<u16>> for AgentStatusClient {
    fn from(port: Option<u16>) -> Self {
        match port {
            Some(port) => AgentStatusClient::Enabled {
                client: Client::new(),
                port,
            },
            None => AgentStatusClient::Disabled,
        }
    }
}

impl AgentStatusClient {
    pub fn is_enabled(&self) -> bool {
        matches!(self, AgentStatusClient::Enabled { .. })
    }

    pub fn status(&self, body: SnarkOSStatus) {
        if let AgentStatusClient::Enabled { client, port } = self {
            let url = format!("http://127.0.0.1:{port}/api/v1/status");
            let client = client.clone();
            thread::spawn(move || {
                let _ = client.post(&url).json(&body).send();
            });
        }
    }

    pub fn post_block<N: Network>(&self, height: u32, blocks: &BlockDB<N>) {
        if let AgentStatusClient::Enabled { client, port } = self {
            // lookup block hash and state root
            let (Ok(Some(block_hash)), Ok(Some(state_root))) =
                (blocks.get_block_hash(height), blocks.get_state_root(height))
            else {
                return;
            };
            // lookup block header
            let Ok(Some(header)) = blocks.get_block_header(&block_hash) else {
                return;
            };

            // assemble the body
            let body = SnarkOSBlockInfo {
                height,
                state_root: state_root.to_string(),
                block_hash: block_hash.to_string(),
                block_timestamp: header.timestamp(),
            };

            let url = format!("http://127.0.0.1:{port}/api/v1/block");
            let client = client.clone();
            thread::spawn(move || {
                let _ = client.post(&url).json(&body).send();
            });
        }
    }
}
