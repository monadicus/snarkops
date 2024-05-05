use std::collections::HashSet;

use snops_common::{
    format::{DataFormat, DataFormatReader},
    state::NodeKey,
};

use crate::{
    cannon::source::{ComputeTarget, CreditsTxMode, LocalService, QueryTarget, TxMode, TxSource},
    schema::nodes::KeySource,
};

#[derive(Debug, Clone)]
pub struct TxSourceFormatHeader {
    pub version: u8,
    pub node_key: <NodeKey as DataFormat>::Header,
    pub key_source: <KeySource as DataFormat>::Header,
}

impl DataFormat for TxSourceFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        Ok(self.version.write_data(writer)?
            + self.node_key.write_data(writer)?
            + self.key_source.write_data(writer)?)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
                "LocalServiceFormatHeader",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        let version = reader.read_data(&())?;
        let node_key = reader.read_data(&((), ()))?;
        let key_source = reader.read_data(&())?;
        Ok(Self {
            version,
            node_key,
            key_source,
        })
    }
}

impl DataFormat for TxSource {
    type Header = TxSourceFormatHeader;
    const LATEST_HEADER: Self::Header = TxSourceFormatHeader {
        version: 1,
        node_key: NodeKey::LATEST_HEADER,
        key_source: KeySource::LATEST_HEADER,
    };

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        let mut written = 0;
        match self {
            TxSource::Playback { file_name } => {
                written += 0u8.write_data(writer)?;
                written += file_name.write_data(writer)?;
            }
            TxSource::RealTime {
                query,
                compute,
                tx_modes,
                private_keys,
                addresses,
            } => {
                written += 1u8.write_data(writer)?;
                match query {
                    QueryTarget::Local(local) => {
                        written += 0u8.write_data(writer)?;
                        written += local.sync_from.write_data(writer)?;
                    }
                    QueryTarget::Node(node) => {
                        written += 1u8.write_data(writer)?;
                        written += node.write_data(writer)?;
                    }
                }

                match compute {
                    ComputeTarget::Agent { labels } => {
                        written += 0u8.write_data(writer)?;
                        written += labels.write_data(writer)?;
                    }
                    ComputeTarget::Demox { demox_api } => {
                        written += 1u8.write_data(writer)?;
                        written += demox_api.write_data(writer)?;
                    }
                }

                written += tx_modes
                    .iter()
                    .map(|mode| match mode {
                        TxMode::Credits(credits) => match credits {
                            CreditsTxMode::BondPublic => 0u8,
                            CreditsTxMode::UnbondPublic => 1u8,
                            CreditsTxMode::TransferPublic => 2u8,
                            CreditsTxMode::TransferPublicToPrivate => 3u8,
                            CreditsTxMode::TransferPrivate => 4u8,
                            CreditsTxMode::TransferPrivateToPublic => 5u8,
                        },
                    })
                    .collect::<Vec<_>>()
                    .write_data(writer)?;
                written += private_keys.write_data(writer)?;
                written += addresses.write_data(writer)?;
            }
            TxSource::Listen { query, compute } => {
                written += 2u8.write_data(writer)?;

                match query {
                    QueryTarget::Local(local) => {
                        written += 0u8.write_data(writer)?;
                        written += local.sync_from.write_data(writer)?;
                    }
                    QueryTarget::Node(node) => {
                        written += 1u8.write_data(writer)?;
                        written += node.write_data(writer)?;
                    }
                }

                match compute {
                    ComputeTarget::Agent { labels } => {
                        written += 0u8.write_data(writer)?;
                        written += labels.write_data(writer)?;
                    }
                    ComputeTarget::Demox { demox_api } => {
                        written += 1u8.write_data(writer)?;
                        written += demox_api.write_data(writer)?;
                    }
                }
            }
        }

        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(snops_common::format::DataReadError::unsupported(
                "TxSource",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        match reader.read_data(&())? {
            0u8 => {
                let file_name = reader.read_data(&())?;
                Ok(TxSource::Playback { file_name })
            }
            1u8 => {
                let query = match reader.read_data(&())? {
                    0u8 => QueryTarget::Local(LocalService {
                        sync_from: reader.read_data(&header.node_key)?,
                    }),
                    1u8 => QueryTarget::Node(reader.read_data(&header.node_key)?),
                    n => {
                        return Err(snops_common::format::DataReadError::Custom(format!(
                            "invalid QueryTarget discriminant: {n}"
                        )));
                    }
                };

                let compute = match reader.read_data(&())? {
                    0u8 => ComputeTarget::Agent {
                        labels: reader.read_data(&())?,
                    },
                    1u8 => ComputeTarget::Demox {
                        demox_api: reader.read_data(&())?,
                    },
                    n => {
                        return Err(snops_common::format::DataReadError::Custom(format!(
                            "invalid ComputeTarget discriminant: {n}"
                        )));
                    }
                };

                let tx_modes = Vec::<u8>::read_data(reader, &())?
                    .into_iter()
                    .map(|n| {
                        Ok(TxMode::Credits(match n {
                            0u8 => CreditsTxMode::BondPublic,
                            1u8 => CreditsTxMode::UnbondPublic,
                            2u8 => CreditsTxMode::TransferPublic,
                            3u8 => CreditsTxMode::TransferPublicToPrivate,
                            4u8 => CreditsTxMode::TransferPrivate,
                            5u8 => CreditsTxMode::TransferPrivateToPublic,
                            n => {
                                return Err(snops_common::format::DataReadError::Custom(format!(
                                    "invalid CreditsTxMode discriminant: {n}"
                                )));
                            }
                        }))
                    })
                    .collect::<Result<HashSet<_>, _>>()?;
                let private_keys = reader.read_data(&header.key_source)?;
                let addresses = reader.read_data(&header.key_source)?;

                Ok(TxSource::RealTime {
                    query,
                    compute,
                    tx_modes,
                    private_keys,
                    addresses,
                })
            }
            2u8 => {
                let query = match reader.read_data(&())? {
                    0u8 => QueryTarget::Local(LocalService {
                        sync_from: reader.read_data(&header.node_key)?,
                    }),
                    1u8 => QueryTarget::Node(reader.read_data(&header.node_key)?),
                    n => {
                        return Err(snops_common::format::DataReadError::Custom(format!(
                            "invalid QueryTarget discriminant: {n}"
                        )));
                    }
                };

                let compute = match reader.read_data(&())? {
                    0u8 => ComputeTarget::Agent {
                        labels: reader.read_data(&())?,
                    },
                    1u8 => ComputeTarget::Demox {
                        demox_api: reader.read_data(&())?,
                    },
                    n => {
                        return Err(snops_common::format::DataReadError::Custom(format!(
                            "invalid ComputeTarget discriminant: {n}"
                        )));
                    }
                };

                Ok(TxSource::Listen { query, compute })
            }
            n => Err(snops_common::format::DataReadError::Custom(format!(
                "invalid TxSource discriminant: {n}"
            ))),
        }
    }
}
