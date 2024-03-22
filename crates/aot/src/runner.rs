use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use aleo_std::StorageMode;
use anyhow::Result;
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_clap_deserialize::serde_clap_default;
use snarkos_node::Node;
use snarkvm::{prelude::Block, utilities::FromBytes};
use snot_common::state::NodeType;

use crate::{ledger::Addrs, Account, PrivateKey};

#[serde_clap_default]
#[derive(Debug, Args, Serialize, Deserialize)]
pub struct Runner {
    /// A path to the genesis block to initialize the ledger from.
    #[arg(required = true, short, long, default_value = "genesis.block")]
    pub genesis: PathBuf,

    /// The ledger from which to view a block.
    #[arg(required = true, short, long, default_value = "./ledger")]
    pub ledger: PathBuf,

    #[arg(required = true, name = "type", short, long)]
    pub node_type: NodeType,

    /// Specify the account private key of the node
    #[clap(long = "private-key")]
    pub private_key: PrivateKey,

    #[clap(long = "bind", default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
    pub bind_addr: IpAddr,
    /// Specify the IP address and port for the node server
    #[clap(long = "node", default_value_t = 4130)]
    pub node: u16,
    /// Specify the IP address and port for the BFT
    #[clap(long = "bft", default_value_t = 5000)]
    pub bft: u16,
    /// Specify the IP address and port for the REST server
    #[clap(long = "rest", default_value_t = 3030)]
    pub rest: u16,

    /// Specify the IP address and port of the peer(s) to connect to
    #[clap(long = "peers")]
    pub peers: Addrs,
    /// Specify the IP address and port of the validator(s) to connect to
    #[clap(long = "validators")]
    pub validators: Addrs,
    /// Specify the requests per second (RPS) rate limit per IP for the REST
    /// server
    #[clap(long = "rest-rps", default_value_t = 1000)]
    pub rest_rps: u32,
}

impl Runner {
    #[tokio::main]
    pub async fn parse(self) -> Result<()> {
        let bind_addr = self.bind_addr;
        let node_ip = SocketAddr::new(bind_addr, self.node);
        let rest_ip = SocketAddr::new(bind_addr, self.rest);
        let bft_ip = SocketAddr::new(bind_addr, self.bft);

        let account = Account::try_from(self.private_key)?;

        let genesis = Block::from_bytes_le(&std::fs::read(&self.genesis)?)?;
        let storage_mode = StorageMode::Custom(self.ledger);

        // snarkos_node::metrics::initialize_metrics();

        match self.node_type {
            NodeType::Validator => {
                Node::new_validator(
                    node_ip,
                    Some(bft_ip),
                    Some(rest_ip),
                    self.rest_rps,
                    account,
                    &self.peers,
                    &self.validators,
                    genesis,
                    None,
                    storage_mode,
                    false,
                    false,
                )
                .await?;
            }
            NodeType::Prover => {
                Node::new_prover(node_ip, account, &self.peers, genesis, storage_mode).await?;
            }
            NodeType::Client => {
                Node::new_client(
                    node_ip,
                    Some(rest_ip),
                    self.rest_rps,
                    account,
                    &self.peers,
                    genesis,
                    None,
                    storage_mode,
                )
                .await?;
            }
        };

        // snarkos will close itself if this is not here...
        std::future::pending::<()>().await;

        Ok(())
    }
}
/*



*/
