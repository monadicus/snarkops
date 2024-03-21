use std::str::FromStr;

use lazy_static::lazy_static;
use regex::Regex;
use serde::{
    de::{Error, Visitor},
    Deserialize,
};
use snot_common::state::{NodeKey, NodeType};

pub mod infrastructure;
pub mod nodes;
pub mod outcomes;
pub mod storage;
pub mod timeline;

// TODO: Considerations:
// TODO: - Generate json schema with https://docs.rs/schemars/latest/schemars/
// TODO: - Do these types need to implement `Serialize`?

/// A document representing all item types.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "version")]
#[non_exhaustive]
pub enum ItemDocument {
    #[serde(rename = "storage.snarkos.testing.monadic.us/v1")]
    Storage(Box<storage::Document>),

    #[serde(rename = "nodes.snarkos.testing.monadic.us/v1")]
    Nodes(Box<nodes::Document>),

    #[serde(rename = "infrastructure.snarkos.testing.monadic.us/v1")]
    Infrastructure(Box<infrastructure::Document>),

    #[serde(rename = "timeline.snarkos.testing.monadic.us/v1")]
    Timeline(Box<timeline::Document>),

    #[serde(rename = "outcomes.snarkos.testing.monadic.us/v1")]
    Outcomes(Box<outcomes::Document>),
}

/// One or more deserialized node targets. Composed of one or more
/// [`NodeTarget`]s.
#[derive(Debug, Clone, Default)]
pub enum NodeTargets {
    #[default]
    None,
    One(NodeTarget),
    Many(Vec<NodeTarget>),
}

struct NodeTargetsVisitor;

impl<'de> Visitor<'de> for NodeTargetsVisitor {
    type Value = NodeTargets;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("one or more node targets")
    }

    fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(NodeTargets::One(FromStr::from_str(v).map_err(E::custom)?))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut buf = vec![];

        while let Some(elem) = seq.next_element()? {
            buf.push(NodeTarget::from_str(elem).map_err(A::Error::custom)?);
        }

        Ok(NodeTargets::Many(buf))
    }
}

impl<'de> Deserialize<'de> for NodeTargets {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(NodeTargetsVisitor)
    }
}

lazy_static! {
    static ref NODE_TARGET_REGEX: Regex =
        Regex::new(r"^(?P<ty>\*|client|validator|prover)\/(?P<id>[A-Za-z0-9\-*]+)(?:@(?P<ns>[A-Za-z0-9\-*]+))?$")
            .unwrap();
}

/// A **single** matched node target. Use [`NodeTargets`] when deserializing
/// from documents.
#[derive(Debug, Clone)]
pub struct NodeTarget {
    pub ty: NodeTargetType,
    pub id: NodeTargetId,
    pub ns: NodeTargetNamespace,
}

impl FromStr for NodeTarget {
    // TODO: enum error for this?
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some(captures) = NODE_TARGET_REGEX.captures(s) else {
            return Err("invalid node target string");
        };

        // match the type
        let ty = match &captures["ty"] {
            "*" => NodeTargetType::All,
            "client" => NodeTargetType::One(NodeType::Client),
            "validator" => NodeTargetType::One(NodeType::Validator),
            "prover" => NodeTargetType::One(NodeType::Prover),
            _ => unreachable!(),
        };

        // match the node ID
        let id = match &captures["id"] {
            // full wildcard
            "*" => NodeTargetId::All,

            // partial wildcard
            id if id.contains('*') => NodeTargetId::WildcardPattern(id.into()),

            // literal string
            id => NodeTargetId::Literal(id.into()),
        };

        // match the namespace
        let ns = match captures.name("ns") {
            // full wildcard
            Some(id) if id.as_str() == "*" => NodeTargetNamespace::All,

            // local; either explicitly stated, or empty
            Some(id) if id.as_str() == "local" => NodeTargetNamespace::Local,
            None => NodeTargetNamespace::Local,

            // literal namespace
            Some(id) => NodeTargetNamespace::Literal(id.as_str().into()),
        };

        Ok(Self { ty, id, ns })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum NodeTargetType {
    /// Matches all node types.
    All,
    /// Matches a particular node type.
    One(NodeType),
}

#[derive(Debug, Clone)]
pub enum NodeTargetId {
    /// `*`. Matches all IDs.
    All,
    /// A wildcard pattern, like `foo-*`.
    WildcardPattern(String),
    /// A literal name, like `foo-node` or `1`.
    Literal(String),
}

#[derive(Debug, Clone)]
pub enum NodeTargetNamespace {
    /// `*`. Matches all namespaces.
    All,
    /// A literal name, like `mainnet`.
    Literal(String),
    /// The local namespace.
    Local,
}

impl From<NodeKey> for NodeTarget {
    fn from(value: NodeKey) -> Self {
        Self {
            ty: NodeTargetType::One(value.ty),
            id: NodeTargetId::Literal(value.id),
            ns: value
                .ns
                .map(NodeTargetNamespace::Literal)
                .unwrap_or(NodeTargetNamespace::Local),
        }
    }
}
