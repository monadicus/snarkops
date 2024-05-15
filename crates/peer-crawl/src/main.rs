use std::{
    collections::HashSet,
    fmt::Display,
    fs::OpenOptions,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use clap::Parser;
use dashmap::{mapref::entry::Entry, DashMap};
use graph::known_nodes_into_graph;
use reqwest::ClientBuilder;
use serde::{Deserialize, Serialize};
use tokio::{select, sync::mpsc, task::JoinSet};

mod graph;

/// A cli tool to gather a live peer topology.
#[derive(Debug, Parser)]
struct Args {
    /// The ip of the first node to scrap.
    #[clap(long, short)]
    initial_ip: Ipv4Addr,
    /// The rest port of all nodes.
    #[clap(long, short, default_value_t = 3030)]
    port: u16,
    /// How often a scrape is performed in millis.
    #[clap(long, short)]
    duration: Option<u64>,
    /// The network id (mainnet, testnet)
    #[clap(long, short, default_value = "mainnet")]
    network: String,
    #[clap(long)]
    graph: Option<PathBuf>,
}

const ENDPOINT: &str = "/peers/all/metrics";

#[derive(Deserialize, Serialize, Copy, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub enum NodeType {
    Client,
    Prover,
    Validator,
}

impl Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Client => f.write_str("Client"),
            Self::Prover => f.write_str("Prover"),
            Self::Validator => f.write_str("Validator"),
        }
    }
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

async fn scrape_and_write(
    initial_peer: SocketAddr,
    network: Arc<String>,
    port: u16,
    graph: Option<PathBuf>,
) -> Result<()> {
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
                    let peers = match get_node_peers(current_peer, &network, port).await {
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
                    for (peer, ty) in peers.into_iter() {
                        // TODO: can we always assume the rest server is on port provided(3030 by
                        // default)?

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

    if let Some(output_graph) = graph {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(output_graph)?;

        serde_json::to_writer(file, &known_nodes_into_graph(&known_nodes))?;
    } else {
        println!("{}", serde_json::to_string(&known_nodes)?);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let initial_peer = SocketAddr::from((args.initial_ip, args.port));
    let network = Arc::new(args.network);

    if let Some(duration) = args.duration {
        let dur = tokio::time::Duration::from_millis(duration);
        // TODO: this is bad persist the data structure
        loop {
            scrape_and_write(initial_peer, network.clone(), args.port, args.graph.clone()).await?;
            println!("-----\nscraped, waiting {duration} ms\n-----");
            tokio::time::sleep(dur).await;
        }
    } else {
        scrape_and_write(initial_peer, network, args.port, args.graph).await?;
    }

    Ok(())
}

type NodePeerEntry = (SocketAddr, NodeType);

async fn get_node_peers(
    mut target: SocketAddr,
    network: &str,
    port: u16,
) -> Result<Vec<NodePeerEntry>> {
    target.set_port(port);

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
