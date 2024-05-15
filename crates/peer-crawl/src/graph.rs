use std::{
    cmp::Ordering,
    collections::HashSet,
    hash::{Hash, Hasher},
    net::SocketAddr,
};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::{NodeType, ScrapedNode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceGraph<T: Hash + Ord, D> {
    pub nodes: Vec<GraphNode<T, D>>,
    pub links: HashSet<GraphEdge<T>>,
}

impl<T: Hash + Ord, D> Default for ForceGraph<T, D> {
    fn default() -> Self {
        Self {
            nodes: Default::default(),
            links: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode<T, D> {
    pub id: T,
    #[serde(flatten)]
    pub data: D,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphEdge<T> {
    pub source: T,
    pub target: T,
}

impl<T: Hash + PartialOrd + Ord> Hash for GraphEdge<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.source.cmp(&self.target) {
            Ordering::Less => {
                self.source.hash(state);
                self.target.hash(state);
            }
            _ => {
                self.target.hash(state);
                self.source.hash(state);
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PeerGraphData {
    pub ty: NodeType,
    pub label: String,
}

pub fn known_nodes_into_graph(
    nodes: &DashMap<SocketAddr, ScrapedNode>,
) -> ForceGraph<SocketAddr, PeerGraphData> {
    let mut graph: ForceGraph<SocketAddr, PeerGraphData> = Default::default();

    for node in nodes {
        let Some(ty) = node.ty else {
            continue;
        };

        let num_peers = node.connected.as_ref().map(|c| c.len()).unwrap_or_default();

        graph.nodes.push(GraphNode {
            id: *node.key(),
            data: PeerGraphData {
                ty,
                label: format!("{} ({ty}, {} peers)", *node.key(), num_peers),
            },
        });

        if let Some(ref peers) = node.connected {
            for peer in peers {
                graph.links.insert(GraphEdge {
                    source: *node.key(),
                    target: *peer,
                });
            }
        }
    }

    graph
}
