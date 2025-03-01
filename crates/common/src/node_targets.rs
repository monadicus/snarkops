use core::fmt;
use std::str::FromStr;

use http::StatusCode;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{
    Deserialize, Serialize,
    de::{Error, Visitor},
    ser::SerializeSeq,
};
use thiserror::Error;
use wildmatch::WildMatch;

use crate::{
    format::*,
    impl_into_status_code,
    state::{NodeKey, NodeType},
};

#[derive(Debug, Error)]
#[error("invalid node target string")]
pub struct NodeTargetError;

impl_into_status_code!(NodeTargetError, |_| StatusCode::BAD_REQUEST);

/// One or more deserialized node targets. Composed of one or more
/// [`NodeTarget`]s.
#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub enum NodeTargets {
    #[default]
    None,
    One(NodeTarget),
    Many(Vec<NodeTarget>),
}

impl DataFormat for NodeTargets {
    type Header = DataHeaderOf<NodeTarget>;
    const LATEST_HEADER: Self::Header = NodeTarget::LATEST_HEADER;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, DataWriteError> {
        match self {
            NodeTargets::None => vec![],
            NodeTargets::One(target) => vec![target.clone()],
            NodeTargets::Many(targets) => targets.clone(),
        }
        .write_data(writer)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, DataReadError> {
        let targets = Vec::<NodeTarget>::read_data(reader, header)?;
        Ok(NodeTargets::from(targets))
    }
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
                if v.contains(',') {
                    return Ok(NodeTargets::Many(
                        v.split(',')
                            .map(|s| NodeTarget::from_str(s.trim()).map_err(E::custom))
                            .collect::<Result<_, _>>()?,
                    ));
                }
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
        Regex::new(r"^(?P<ty>\*|any|client|validator|prover)\/(?P<id>[A-Za-z0-9\-*]+)(?:@(?P<ns>[A-Za-z0-9\-*]+))?$")
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

impl fmt::Display for NodeTargets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeTargets::None => write!(f, ""),
            NodeTargets::One(target) => write!(f, "{target}"),
            NodeTargets::Many(targets) => {
                let mut iter = targets.iter();
                if let Some(target) = iter.next() {
                    write!(f, "{target}")?;
                    for target in iter {
                        write!(f, ", {target}")?;
                    }
                }
                Ok(())
            }
        }
    }
}

impl NodeTargets {
    pub const ALL: Self = Self::One(NodeTarget::ALL);

    pub fn is_all(&self) -> bool {
        if matches!(self, NodeTargets::One(NodeTarget::ALL)) {
            return true;
        }

        if let NodeTargets::Many(targets) = self {
            return targets.iter().any(|target| target == &NodeTarget::ALL);
        }

        false
    }
}

/// A **single** matched node target. Use [`NodeTargets`] when deserializing
/// from documents.
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub struct NodeTarget {
    pub ty: NodeTargetType,
    pub id: NodeTargetId,
    pub ns: NodeTargetNamespace,
}

impl FromStr for NodeTarget {
    type Err = NodeTargetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let captures = NODE_TARGET_REGEX.captures(s).ok_or(NodeTargetError)?;

        // match the type
        let ty = match &captures["ty"] {
            "*" => NodeTargetType::All,
            "any" => NodeTargetType::All,
            "client" => NodeTargetType::One(NodeType::Client),
            "validator" => NodeTargetType::One(NodeType::Validator),
            "prover" => NodeTargetType::One(NodeType::Prover),
            _ => unreachable!(),
        };

        // match the node ID
        let id = match &captures["id"] {
            // full wildcard
            "*" => NodeTargetId::All,
            "any" => NodeTargetId::All,

            // partial wildcard
            id if id.contains('*') => NodeTargetId::WildcardPattern(WildMatch::new(id)),

            // literal string
            id => NodeTargetId::Literal(id.into()),
        };

        // match the namespace
        let ns = match captures.name("ns") {
            // full wildcard
            Some(id) if id.as_str() == "*" => NodeTargetNamespace::All,
            Some(id) if id.as_str() == "any" => NodeTargetNamespace::All,

            // local; either explicitly stated, or empty
            Some(id) if id.as_str() == "local" => NodeTargetNamespace::Local,
            None => NodeTargetNamespace::Local,

            // literal namespace
            Some(id) => NodeTargetNamespace::Literal(id.as_str().into()),
        };

        Ok(Self { ty, id, ns })
    }
}

impl fmt::Display for NodeTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}{}",
            match self.ty {
                NodeTargetType::All => "any".to_owned(),
                NodeTargetType::One(ty) => ty.to_string(),
            },
            match &self.id {
                NodeTargetId::All => "any".to_owned(),
                NodeTargetId::WildcardPattern(pattern) => pattern.to_string(),
                NodeTargetId::Literal(id) => id.to_owned(),
            },
            match &self.ns {
                NodeTargetNamespace::All => "@any".to_owned(),
                NodeTargetNamespace::Local => "".to_owned(),
                NodeTargetNamespace::Literal(ns) => format!("@{}", ns),
            }
        )
    }
}

impl Serialize for NodeTarget {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for NodeTarget {
    fn deserialize<D>(deserializer: D) -> Result<NodeTarget, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NodeTarget::from_str(&s).map_err(D::Error::custom)
    }
}

impl DataFormat for NodeTarget {
    type Header = (u8, DataHeaderOf<NodeType>);
    const LATEST_HEADER: Self::Header = (1, NodeType::LATEST_HEADER);

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += match self.ty {
            NodeTargetType::All => 0u8.write_data(writer)?,
            NodeTargetType::One(ty) => 1u8.write_data(writer)? + ty.write_data(writer)?,
        };
        written += match &self.id {
            NodeTargetId::All => 0u8.write_data(writer)?,
            NodeTargetId::WildcardPattern(pattern) => {
                1u8.write_data(writer)? + pattern.to_string().write_data(writer)?
            }
            NodeTargetId::Literal(id) => 2u8.write_data(writer)? + id.write_data(writer)?,
        };
        written += match &self.ns {
            NodeTargetNamespace::All => 0u8.write_data(writer)?,
            NodeTargetNamespace::Local => 1u8.write_data(writer)?,
            NodeTargetNamespace::Literal(ns) => 2u8.write_data(writer)? + ns.write_data(writer)?,
        };

        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, DataReadError> {
        if header.0 != Self::LATEST_HEADER.0 {
            return Err(DataReadError::unsupported(
                "NodeTarget",
                Self::LATEST_HEADER.0,
                header.0,
            ));
        }

        let ty = match reader.read_data(&())? {
            0u8 => NodeTargetType::All,
            1u8 => NodeTargetType::One(NodeType::read_data(reader, &header.1)?),
            n => {
                return Err(DataReadError::Custom(format!(
                    "invalid NodeTarget type discriminant: {n}"
                )));
            }
        };

        let id = match reader.read_data(&())? {
            0u8 => NodeTargetId::All,
            1u8 => {
                let pattern = String::read_data(reader, &())?;
                NodeTargetId::WildcardPattern(WildMatch::new(&pattern))
            }
            2u8 => NodeTargetId::Literal(reader.read_data(&())?),
            n => {
                return Err(DataReadError::Custom(format!(
                    "invalid NodeTarget ID discriminant: {n}"
                )));
            }
        };

        let ns = match reader.read_data(&())? {
            0u8 => NodeTargetNamespace::All,
            1u8 => NodeTargetNamespace::Local,
            2u8 => NodeTargetNamespace::Literal(reader.read_data(&())?),
            n => {
                return Err(DataReadError::Custom(format!(
                    "invalid NodeTarget namespace discriminant: {n}"
                )));
            }
        };

        Ok(Self { ty, id, ns })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeTargetType {
    /// Matches all node types.
    All,
    /// Matches a particular node type.
    One(NodeType),
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeTargetId {
    /// `*`. Matches all IDs.
    All,
    /// A wildcard pattern, like `foo-*`.
    WildcardPattern(WildMatch),
    /// A literal name, like `foo-node` or `1`.
    Literal(String),
}

impl Eq for NodeTargetId {}

impl std::hash::Hash for NodeTargetId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            NodeTargetId::All => "*".hash(state),
            NodeTargetId::WildcardPattern(pattern) => pattern.to_string().hash(state),
            NodeTargetId::Literal(id) => id.hash(state),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Eq, Serialize, Deserialize)]
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

impl From<Vec<NodeTarget>> for NodeTargets {
    fn from(nodes: Vec<NodeTarget>) -> Self {
        match nodes.len() {
            0 => Self::None,
            1 => Self::One(nodes.into_iter().next().unwrap()),
            _ => Self::Many(nodes),
        }
    }
}

impl NodeTarget {
    pub const ALL: Self = Self {
        ty: NodeTargetType::All,
        id: NodeTargetId::All,
        ns: NodeTargetNamespace::All,
    };

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
                ns == "local" && key.ns.is_none() || (key.ns.as_ref() == Some(ns))
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
