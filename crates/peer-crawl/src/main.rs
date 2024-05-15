use std::{
    collections::HashSet,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use clap::Parser;
use dashmap::{mapref::entry::Entry, DashMap};
use reqwest::ClientBuilder;
use serde::{Deserialize, Serialize};
use tokio::{select, sync::mpsc, task::JoinSet};

/// A cli tool to gather a live peer topology.
#[derive(Debug, Parser)]
struct Args {
    /// The ip of the first node to scrap.
    #[clap(long, short)]
    initial_ip: Ipv4Addr,
    /// The rest port of all nodes.
    #[clap(long, short, default_value_t = 3030)]
    port: u16,
    // /// How often a scrape is performed
    // #[clap(long, short, default_value_t = 15_000)]
    // duration: u64,
    /// The network id (mainnet, testnet)
    #[clap(long, short, default_value = "mainnet")]
    network: String,
}

const ENDPOINT: &str = "/peers/all/metrics";

#[derive(Deserialize, Serialize, Copy, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
enum NodeType {
    Client,
    Prover,
    Validator,
}

#[derive(Serialize, Debug)]
struct ScrapedNode {
    ty: Option<NodeType>,
    connected: Option<HashSet<SocketAddr>>,
    #[serde(skip)]
    attempted: bool,
    /* TODO: can make this more memory-efficient with a
     * undirected graph */
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let initial_peer = SocketAddr::from((args.initial_ip, args.port));
    let network = Arc::new(args.network);

    // TODO: use duration so we have persistence

    let known_nodes: Arc<DashMap<SocketAddr, ScrapedNode>> = Default::default();
    let (peers_tx, mut peers_rx) = mpsc::unbounded_channel();
    let mut queue = JoinSet::new();

    peers_tx.send(initial_peer).unwrap();

    'peers: loop {
        select! {
            biased;

            // first check to see if a new peer is available
            Some(current_peer) = peers_rx.recv() => {
                match known_nodes.entry(current_peer) {
                    Entry::Vacant(ent) => {
                        ent.insert(ScrapedNode {
                            ty: None,
                            connected: None,
                            attempted: true,
                        });
                    }

                    // skip this peer if we've already tried hitting it
                    Entry::Occupied(mut ent) => {
                        let node = ent.get_mut();
                        if node.attempted {
                            println!("Skipping already seen peer: {current_peer}");
                            continue 'peers;
                        }
                        node.attempted = true;
                    }
                };

                println!("{current_peer}: attempting to scrape...");

                let peers_tx = peers_tx.clone();
                let network = Arc::clone(&network);
                let known_nodes = Arc::clone(&known_nodes);

                queue.spawn(async move {
                    let peers = match get_node_peers(current_peer, &network).await {
                        Ok(peers) => peers,
                        Err(e) => {
                            eprintln!("{current_peer}: error: {e}");
                            return;
                        }
                    };

                    println!("{current_peer}: scraped successfully");

                    // mark node as known
                    known_nodes.get_mut(&current_peer).unwrap().connected =
                        Some(peers.iter().map(|(addr, _)| *addr).collect());

                    // iterate over each peer
                    for (mut peer, ty) in peers.into_iter() {
                        // TODO: can we always assume the rest server is on port provided(3030 by
                        // default)?
                        peer.set_port(args.port);

                        // mark this node's type as known for after we hit it
                        let entry = known_nodes.entry(peer);
                        let node = entry
                            .and_modify(|ent| ent.ty = Some(ty))
                            .or_insert_with(|| ScrapedNode {
                                ty: Some(ty),
                                connected: None,
                                attempted: false,
                            });

                        if node.attempted {
                            continue;
                        }

                        // enqueue it
                        peers_tx.send(peer).unwrap();
                    }
                });
            }

            result = queue.join_next() => {
                if result.is_none() {
                    break 'peers;
                }
            }
        }
    }

    println!("{}", serde_json::to_string(&known_nodes)?);

    Ok(())
}

type NodePeerEntry = (SocketAddr, NodeType);

async fn get_node_peers(target: SocketAddr, network: &str) -> Result<Vec<NodePeerEntry>> {
    let client = ClientBuilder::new()
        .timeout(Duration::from_millis(2_000))
        .build()?;
    Ok(client
        .get(format!("http://{target}/{network}{ENDPOINT}"))
        .send()
        .await?
        .json()
        .await?)
}
