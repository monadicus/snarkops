use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr},
};

use fixedbitset::FixedBitSet;
use indexmap::IndexMap;
use serde::{de::Visitor, Deserialize, Deserializer, Serialize};
use snops_common::{
    key_source::KeySource,
    lasso::Spur,
    node_targets::NodeTargets,
    set::{MaskBit, MASK_PREFIX_LEN},
    state::{AgentId, DocHeightRequest, NetworkId, NodeState},
    INTERN,
};

use super::NodeKey;
use crate::persist::prelude::*;

/// A document describing the node infrastructure for a test.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub name: String,
    pub description: Option<String>,
    /// The network to use for all nodes.
    ///
    /// Determines if /mainnet/ or /testnet/ are used in routes.
    ///
    /// Also determines which parameters/genesis block to use
    pub network: Option<NetworkId>,

    #[serde(default)]
    pub external: IndexMap<NodeKey, ExternalNode>,

    #[serde(default)]
    pub nodes: IndexMap<NodeKey, Node>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalNode {
    // NOTE: these fields must be validated at runtime, because validators require `bft` to be set,
    // and non-validators require `node` to be set
    // rest is required to be a target of the tx-cannon
    pub bft: Option<SocketAddr>,
    pub node: Option<SocketAddr>,
    pub rest: Option<SocketAddr>,
}

impl DataFormat for ExternalNode {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += writer.write_data(&self.bft)?;
        written += writer.write_data(&self.node)?;
        written += writer.write_data(&self.rest)?;
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, DataReadError> {
        match header {
            1 => {
                let bft = reader.read_data(&())?;
                let node = reader.read_data(&())?;
                let rest = reader.read_data(&())?;
                Ok(ExternalNode { bft, node, rest })
            }
            _ => Err(DataReadError::Custom("unsupported version".to_owned())),
        }
    }
}

/// Impl serde Deserialize ExternalNode but allow for { bft: addr, node: addr,
/// rest: addr} or just `addr`
impl<'de> Deserialize<'de> for ExternalNode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ExternalNodeVisitor;

        impl<'de> Visitor<'de> for ExternalNodeVisitor {
            type Value = ExternalNode;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an ip address or a map of socket addresses")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut bft = None;
                let mut node = None;
                let mut rest = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "bft" => {
                            bft = Some(map.next_value()?);
                        }
                        "node" => {
                            node = Some(map.next_value()?);
                        }
                        "rest" => {
                            rest = Some(map.next_value()?);
                        }
                        _ => {
                            return Err(serde::de::Error::unknown_field(
                                &key,
                                &["bft", "node", "rest"],
                            ));
                        }
                    }
                }

                Ok(ExternalNode { bft, node, rest })
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let ip: IpAddr = v.parse().map_err(E::custom)?;
                Ok(ExternalNode {
                    bft: Some(SocketAddr::new(ip, 5000)),
                    node: Some(SocketAddr::new(ip, 4130)),
                    rest: Some(SocketAddr::new(ip, 3030)),
                })
            }
        }

        deserializer.deserialize_any(ExternalNodeVisitor)
    }
}

// zander forgive me -isaac
fn please_be_online() -> bool {
    true
}

/// Parse the labels as strings, but intern them on load
pub fn deser_label<'de, D>(deserializer: D) -> Result<HashSet<Spur>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let labels = Vec::<String>::deserialize(deserializer)?;
    Ok(labels
        .into_iter()
        .map(|label| INTERN.get_or_intern(label))
        .collect())
}

fn ser_label<S>(labels: &HashSet<Spur>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let labels: Vec<&str> = labels.iter().map(|key| INTERN.resolve(key)).collect();
    labels.serialize(serializer)
}

// TODO: could use some more clarification on some of these fields
/// A node in the testing infrastructure.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Node {
    #[serde(default = "please_be_online")]
    pub online: bool,
    /// When specified, creates a group of nodes, all with the same
    /// configuration.
    pub replicas: Option<usize>,
    /// The private key to start the node with.
    pub key: Option<KeySource>,
    /// Height of ledger to inherit.
    ///
    /// * When null, a ledger is created when the node is started.
    /// * When zero, the ledger is empty and only the genesis block is
    ///   inherited.
    #[serde(default)]
    pub height: DocHeightRequest,

    /// When specified, agents must have these labels
    #[serde(
        default,
        deserialize_with = "deser_label",
        serialize_with = "ser_label"
    )]
    pub labels: HashSet<Spur>,

    /// When specified, an agent must have this id. Overrides the labels field.
    #[serde(default)]
    pub agent: Option<AgentId>,

    /// List of validators for the node to connect to
    #[serde(default)]
    pub validators: NodeTargets,

    /// List of peers for the node to connect to
    #[serde(default)]
    pub peers: NodeTargets,

    /// Environment variables to inject into the snarkOS process.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Node {
    pub fn into_state(&self, node_key: NodeKey) -> NodeState {
        NodeState {
            node_key,
            private_key: Default::default(),
            height: (0, self.height.into()),
            online: self.online,
            env: self.env.clone(),

            // these are resolved later
            validators: Default::default(),
            peers: Default::default(),
        }
    }

    pub fn mask(&self, key: &NodeKey, labels: &[Spur]) -> FixedBitSet {
        let mut mask = FixedBitSet::with_capacity(labels.len() + MASK_PREFIX_LEN);

        // validator/prover/client
        mask.insert(key.ty.bit());

        // local private key
        if matches!(self.key, Some(KeySource::Local)) {
            mask.insert(MaskBit::LocalPrivateKey as usize);
        }

        // labels
        for (i, label) in labels.iter().enumerate() {
            if self.labels.contains(label) {
                mask.insert(i + MASK_PREFIX_LEN);
            }
        }
        mask
    }
}

#[derive(Debug, Clone)]
pub struct NodeFormatHeader {
    pub(crate) key_source: DataHeaderOf<KeySource>,
    pub(crate) height_request: DataHeaderOf<DocHeightRequest>,
    pub(crate) node_targets: DataHeaderOf<NodeTargets>,
}

impl DataFormat for NodeFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += self.key_source.write_data(writer)?;
        written += self.height_request.write_data(writer)?;
        written += self.node_targets.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, DataReadError> {
        match header {
            1 => {
                let key_source = KeySource::read_header(reader)?;
                let height_request = DocHeightRequest::read_header(reader)?;
                let node_targets = NodeTargets::read_header(reader)?;
                Ok(NodeFormatHeader {
                    key_source,
                    height_request,
                    node_targets,
                })
            }
            _ => Err(DataReadError::unsupported(
                "NodeFormatHeader",
                Self::LATEST_HEADER,
                *header,
            )),
        }
    }
}

impl DataFormat for Node {
    type Header = NodeFormatHeader;
    const LATEST_HEADER: Self::Header = NodeFormatHeader {
        key_source: KeySource::LATEST_HEADER,
        height_request: DocHeightRequest::LATEST_HEADER,
        node_targets: NodeTargets::LATEST_HEADER,
    };

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += self.online.write_data(writer)?;
        written += self.replicas.write_data(writer)?;
        written += self.key.write_data(writer)?;
        written += self.height.write_data(writer)?;
        written += self.labels.write_data(writer)?;
        written += self.agent.write_data(writer)?;
        written += self.validators.write_data(writer)?;
        written += self.peers.write_data(writer)?;
        written += self.env.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, DataReadError> {
        let online = reader.read_data(&())?;
        let replicas = reader.read_data(&())?;
        let key = reader.read_data(&header.key_source)?;
        let height = reader.read_data(&header.height_request)?;
        let labels = Vec::<Spur>::read_data(reader, &())?;
        let agent = reader.read_data(&())?;
        let validators = reader.read_data(&header.node_targets)?;
        let peers = reader.read_data(&header.node_targets)?;
        let env = Vec::<(String, String)>::read_data(reader, &((), ()))?;

        Ok(Node {
            online,
            replicas,
            key,
            height,
            labels: labels.into_iter().collect(),
            agent,
            validators,
            peers,
            env: env.into_iter().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use snops_common::key_source::ACCOUNTS_KEY_ID;

    use super::*;

    #[test]
    fn test_key_source_deserialization() {
        assert_eq!(
            serde_yaml::from_str::<KeySource>("committee.0").unwrap(),
            KeySource::Committee(Some(0))
        );
        assert_eq!(
            serde_yaml::from_str::<KeySource>("committee.100").unwrap(),
            KeySource::Committee(Some(100))
        );
        assert_eq!(
            serde_yaml::from_str::<KeySource>("committee.$").unwrap(),
            KeySource::Committee(None)
        );

        assert_eq!(
            serde_yaml::from_str::<KeySource>("accounts.0").unwrap(),
            KeySource::Named(*ACCOUNTS_KEY_ID, Some(0))
        );
        assert_eq!(
            serde_yaml::from_str::<KeySource>("accounts.$").unwrap(),
            KeySource::Named(*ACCOUNTS_KEY_ID, None)
        );

        assert_eq!(
            serde_yaml::from_str::<KeySource>(
                "APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH"
            )
            .unwrap(),
            KeySource::PrivateKeyLiteral(
                "APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH".to_string()
            )
        );

        assert_eq!(
            serde_yaml::from_str::<KeySource>(
                "aleo1ekc03f2vwemtpksckhrcl7mv4t7sm6ykldwldvvlysqt2my9zygqfhndya"
            )
            .unwrap(),
            KeySource::PublicKeyLiteral(
                "aleo1ekc03f2vwemtpksckhrcl7mv4t7sm6ykldwldvvlysqt2my9zygqfhndya".to_string()
            )
        );

        assert!(serde_yaml::from_str::<KeySource>("committee.-100").is_err(),);
        assert!(serde_yaml::from_str::<KeySource>("accounts.-100").is_err(),);
        assert!(serde_yaml::from_str::<KeySource>("accounts._").is_err(),);
        assert!(serde_yaml::from_str::<KeySource>("accounts.*").is_err(),);
    }
}
