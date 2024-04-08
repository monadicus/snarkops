use std::{fmt::Display, str::FromStr};

use lazy_static::lazy_static;
use regex::Regex;
use serde::{
    de::{Error, Visitor},
    ser::SerializeSeq,
    Deserialize, Serialize,
};
use snops_common::state::{NodeKey, NodeType};
use wildmatch::WildMatch;

use self::error::{NodeTargetError, SchemaError};

pub mod cannon;
pub mod error;
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

    #[serde(rename = "cannon.snarkos.testing.monadic.us/v1")]
    Cannon(Box<cannon::Document>),
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

impl<'de> Deserialize<'de> for NodeTargets {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
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

                Ok(if buf.is_empty() {
                    NodeTargets::None
                } else {
                    NodeTargets::Many(buf)
                })
            }
        }

        deserializer.deserialize_any(NodeTargetsVisitor)
    }
}

lazy_static! {
    static ref NODE_TARGET_REGEX: Regex =
        Regex::new(r"^(?P<ty>\*|client|validator|prover)\/(?P<id>[A-Za-z0-9\-*]+)(?:@(?P<ns>[A-Za-z0-9\-*]+))?$")
            .unwrap();
}

impl Serialize for NodeTargets {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            NodeTargets::None => serializer.serialize_seq(Some(0))?.end(),
            NodeTargets::One(target) => serializer.serialize_str(&target.to_string()),
            NodeTargets::Many(targets) => {
                let mut seq = serializer.serialize_seq(Some(targets.len()))?;
                for target in targets {
                    seq.serialize_element(&target.to_string())?;
                }
                seq.end()
            }
        }
    }
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
    type Err = SchemaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let captures = NODE_TARGET_REGEX.captures(s).ok_or(NodeTargetError)?;

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
            id if id.contains('*') => NodeTargetId::WildcardPattern(WildMatch::new(id)),

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

impl Display for NodeTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/{}{}",
            match self.ty {
                NodeTargetType::All => "*".to_owned(),
                NodeTargetType::One(ty) => ty.to_string(),
            },
            match &self.id {
                NodeTargetId::All => "*".to_owned(),
                NodeTargetId::WildcardPattern(pattern) => pattern.to_string(),
                NodeTargetId::Literal(id) => id.to_owned(),
            },
            match &self.ns {
                NodeTargetNamespace::All => "@*".to_owned(),
                NodeTargetNamespace::Local => "".to_owned(),
                NodeTargetNamespace::Literal(ns) => format!("@{}", ns),
            }
        )
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
    WildcardPattern(WildMatch),
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

impl NodeTarget {
    pub fn matches(&self, key: &NodeKey) -> bool {
        (match self.ty {
            NodeTargetType::All => true,
            NodeTargetType::One(ty) => ty == key.ty,
        }) && (match &self.id {
            NodeTargetId::All => true,
            NodeTargetId::WildcardPattern(pattern) => pattern.matches(&key.id),
            NodeTargetId::Literal(id) => &key.id == id,
        }) && (match &self.ns {
            NodeTargetNamespace::All => true,
            NodeTargetNamespace::Local => key.ns.is_none() || key.ns == Some("local".into()),
            NodeTargetNamespace::Literal(ns) => {
                ns == "local" && key.ns.is_none()
                    || key.ns.as_ref().map_or(false, |key_ns| key_ns == ns)
            }
        })
    }
}

impl NodeTargets {
    pub fn is_empty(&self) -> bool {
        if matches!(self, &NodeTargets::None) {
            return true;
        }

        if let NodeTargets::Many(targets) = self {
            return targets.is_empty();
        }

        false
    }

    pub fn matches(&self, key: &NodeKey) -> bool {
        match self {
            NodeTargets::None => false,
            NodeTargets::One(target) => target.matches(key),
            NodeTargets::Many(targets) => targets.iter().any(|target| target.matches(key)),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::env::Environment;

    #[test]
    fn deserialize_specs() {
        for entry in std::fs::read_dir("../../specs")
            .expect("failed to read specs dir")
            .map(Result::unwrap)
        {
            let file_name = entry.file_name();
            let name = file_name.to_str().expect("failed to read spec file name");
            if !name.ends_with(".yaml") && !name.ends_with(".yml") {
                continue;
            }

            let data = std::fs::read(entry.path()).expect("failed to read spec file");
            if let Err(e) = Environment::deserialize_bytes(&data) {
                panic!("failed to deserialize spec file {name}: {e}")
            }
        }
    }
}
