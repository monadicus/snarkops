use std::{
    net::{Ipv4Addr, SocketAddr},
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
    #[arg(required = true, short, long)]
    #[serde_clap_default(PathBuf::from("genesis.block"))]
    pub genesis: PathBuf,

    /// The ledger from which to view a block.
    #[arg(required = true, short, long)]
    #[serde_clap_default(PathBuf::from("./ledger"))]
    pub ledger: PathBuf,

    #[arg(required = true, name = "type", short, long)]
    pub node_type: NodeType,

    /// Specify the account private key of the node
    #[clap(long = "private-key")]
    pub private_key: PrivateKey,

    /// Specify the IP address and port for the node server
    #[clap(long = "node")]
    #[serde_clap_default(4130)]
    pub node: u16,
    /// Specify the IP address and port for the BFT
    #[clap(long = "bft")]
    #[serde_clap_default(Some(5000))]
    pub bft: Option<u16>,
    /// Specify the IP address and port of the peer(s) to connect to
    #[clap(long = "peers")]
    #[serde_clap_default(Default::default())]
    pub peers: Addrs,
    /// Specify the IP address and port of the validator(s) to connect to
    #[clap(long = "validators")]
    #[serde_clap_default(Default::default())]
    pub validators: Addrs,
    /// Specify the IP address and port for the REST server
    #[clap(long = "rest")]
    #[serde_clap_default(Some(3030))]
    pub rest: Option<u16>,
    /// Specify the requests per second (RPS) rate limit per IP for the REST
    /// server
    #[clap(long = "rest-rps")]
    #[serde_clap_default(1000)]
    pub rest_rps: u32,
}

impl Runner {
    #[tokio::main]
    pub async fn parse(self) -> Result<()> {
        let node_ip = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), self.node);
        let rest_ip = self
            .rest
            .map(|port| SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port));
        let bft_ip = self
            .bft
            .map(|port| SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port));

        let account = Account::try_from(self.private_key)?;

        let genesis = Block::from_bytes_le(&std::fs::read(&self.genesis)?)?;
        let storage_mode = StorageMode::Custom(self.genesis);

        // snarkos_node::metrics::initialize_metrics();

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
                .await?;
            }
            NodeType::Prover => {
                Node::new_prover(node_ip, account, &self.peers, genesis, storage_mode).await?;
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
                .await?;
            }
        };

        Ok(())
    }
}
/*



*/
