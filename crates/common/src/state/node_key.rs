use std::{fmt::Write, str::FromStr};

use serde::de::Error;

use super::{NodeType, NODE_KEY_REGEX};
use crate::format::{DataFormat, DataFormatReader, DataFormatWriter, DataHeaderOf};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct NodeKey {
    pub ty: NodeType,
    pub id: String,
    /// The node key namespace. If `None`, is a local node.
    pub ns: Option<String>, // TODO: string interning or otherwise not duplicating namespace
}

impl FromStr for NodeKey {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some(captures) = NODE_KEY_REGEX.captures(s) else {
            return Err("invalid node key string");
        };

        // match the type
        let ty = NodeType::from_str(&captures["ty"]).unwrap();

        // match the node ID
        let id = captures
            .name("id")
            .map(|id| id.as_str().to_string())
            .unwrap_or_default();

        // match the namespace
        let ns = match captures.name("ns") {
            // local; either explicitly stated, or empty
            Some(id) if id.as_str() == "local" => None,
            None => None,

            // literal namespace
            Some(id) => Some(id.as_str().into()),
        };

        Ok(Self { ty, id, ns })
    }
}

impl<'de> serde::Deserialize<'de> for NodeKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(D::Error::custom)
    }
}

impl std::fmt::Display for NodeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.ty, self.id)?;
        if let Some(ns) = &self.ns {
            f.write_char('@')?;
            f.write_str(ns)?;
        }

        Ok(())
    }
}

impl serde::Serialize for NodeKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl DataFormat for NodeKey {
    type Header = (u8, DataHeaderOf<NodeType>);
    const LATEST_HEADER: Self::Header = (1, NodeType::LATEST_HEADER);

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        let mut written = 0;
        written += writer.write_data(&self.ty)?;
        written += writer.write_data(&self.id)?;
        written += writer.write_data(&self.ns)?;
        Ok(written)
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if header.0 != Self::LATEST_HEADER.0 {
            return Err(crate::format::DataReadError::unsupported(
                "NodeKey",
                Self::LATEST_HEADER.0,
                header.0,
            ));
        }

        let ty = reader.read_data(&header.1)?;
        let id = reader.read_data(&())?;
        let ns = reader.read_data(&())?;

        Ok(Self { ty, id, ns })
    }
}

#[cfg(test)]
mod test {
    use crate::state::{NodeKey, NodeType::*};

    #[test]
    fn test_node_key_parse() {
        use super::NodeKey;

        let key = NodeKey {
            ty: Client,
            id: "test".to_string(),
            ns: None,
        };

        let s = key.to_string();
        assert_eq!(s, "client/test");

        let key2 = s.parse::<NodeKey>().unwrap();
        assert_eq!(key, key2);

        let key = NodeKey {
            ty: Client,
            id: "test".to_string(),
            ns: Some("ns".to_string()),
        };

        let s = key.to_string();
        assert_eq!(s, "client/test@ns");

        let key2 = s.parse::<NodeKey>().unwrap();
        assert_eq!(key, key2);
    }

    #[test]
    fn test_node_key_serde() {
        assert_eq!(
            serde_yaml::from_str::<NodeKey>("client").unwrap(),
            NodeKey {
                ty: Client,
                id: "".to_string(),
                ns: None
            }
        );
        assert_eq!(
            serde_yaml::from_str::<NodeKey>("validator/foo").unwrap(),
            NodeKey {
                ty: Validator,
                id: "foo".to_string(),
                ns: None
            }
        );
        assert_eq!(
            serde_yaml::from_str::<NodeKey>("validator@foo").unwrap(),
            NodeKey {
                ty: Validator,
                id: "".to_string(),
                ns: Some("foo".to_string())
            }
        );
        assert_eq!(
            serde_yaml::from_str::<NodeKey>("client/foo@bar").unwrap(),
            NodeKey {
                ty: Client,
                id: "foo".to_string(),
                ns: Some("bar".to_string())
            }
        );

        assert!(serde_yaml::from_str::<NodeKey>("client@").is_err());
        assert!(serde_yaml::from_str::<NodeKey>("unknown@").is_err());
        assert!(serde_yaml::from_str::<NodeKey>("unknown").is_err());
        assert!(serde_yaml::from_str::<NodeKey>("client@@").is_err());
        assert!(serde_yaml::from_str::<NodeKey>("validator/!").is_err());
        assert!(serde_yaml::from_str::<NodeKey>("client/!").is_err());
    }
}
