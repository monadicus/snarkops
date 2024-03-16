use aleo_std::StorageMode;
use anyhow::Result;
use serde::Deserialize;
use snarkos_node::Node;
use snarkvm::ledger::Block;
use snot_common::state::NodeType;
use std::{
    future, net::{Ipv4Addr, SocketAddr}, path::PathBuf
};

use crate::{ledger::Addrs, Account, PrivateKey};

#[derive(Debug, Args, Serialize, Deserialize)]
pub struct Runner {
    /// A path to the genesis block to initialize the ledger from.
    #[arg(required = true, short, long, default_value = "./genesis.block")]
    pub genesis: PathBuf,

    /// The ledger from which to view a block.
    #[arg(required = true, short, long, default_value = "./ledger")]
    pub ledger: PathBuf,

    #[arg(required = true, name = "type", short, long)]
    pub node_type: NodeType,

    /// Specify the account private key of the node
    #[clap(long = "private-key")]
    pub private_key: Option<PrivateKey>,

    /// Specify the IP address and port for the node server
    #[clap(default_value_t = 4130, long = "node")]
    pub node: u16,
    /// Specify the IP address and port for the BFT
    #[clap(long = "bft", default_value_t = Some(5000))]
    pub bft: Option<u16>,
    /// Specify the IP address and port of the peer(s) to connect to
    #[clap(default_value = "", long = "peers")]
    pub peers: Addrs,
    /// Specify the IP address and port of the validator(s) to connect to
    #[clap(default_value = "", long = "validators")]
    pub validators: Addrs,
    /// Specify the IP address and port for the REST server
    #[clap(default_value_t = Some(3030), long = "rest")]
    pub rest: Option<u16>,
    /// Specify the requests per second (RPS) rate limit per IP for the REST server
    #[clap(default_value_t = Some(1000), long = "rest-rps")]
    pub rest_rps: u32,
}

impl Runner {
    pub fn parse(self) -> Result<()> {
        let node_ip = SocketAddr::new(Ipv4Addr::UNSPECIFIED, self.node);
        let rest_ip = self
            .rest
            .map(|port| SocketAddr::new(Ipv4Addr::UNSPECIFIED, port));
        let bft_ip = self
            .bft
            .map(|port| SocketAddr::new(Ipv4Addr::UNSPECIFIED, port));

        let account = Account::try_from(private_key)?;

        let storage_mode = StorageMode::Custom(self.genesis);
        let genesis = Block::from_bytes_le(&std::fs::read(&self.genesis)?)?;

        snarkos_node::metrics::initialize_metrics();

        match self.node_type {
            NodeType::Validator => {
                Node::new_validator(
                    node_ip,
                    bft_ip,
                    rest_ip,
                    self.rest_rps,
                    account,
                    &self.peers,
                    &self.validators,
                    genesis,
                    None,
                    storage_mode,
                    false,
                )
            }
            NodeType::Prover => {
                Node::new_prover(node_ip, account, &self.peers, genesis, storage_mod
            }
            NodeType::Client => {
                Node::new_client(
                    node_ip,
                    rest_ip,
                    self.rest_rps,
                    account,
                    &self.peers,
                    genesis,
                    None,
                    storage_mode,
                )
            }
        }
    }
}
/*



*/
