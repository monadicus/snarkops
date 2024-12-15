use std::sync::Arc;

use snops_common::{
    schema::cannon::{sink::TxSink, source::TxSource},
    state::{CannonId, EnvId, NetworkId, NodeKey, StorageId, TransactionSendState},
};
use tokio::sync::Semaphore;

use super::prelude::*;
use crate::{
    cannon::tracker::TransactionTracker,
    env::{
        error::{EnvError, PrepareError},
        prepare_cannons, EnvNode, Environment,
    },
    state::GlobalState,
};

#[derive(Clone)]
pub struct PersistEnvFormatHeader {
    version: u8,
    nodes: DataHeaderOf<EnvNode>,
    tx_source: DataHeaderOf<TxSource>,
    tx_sink: DataHeaderOf<TxSink>,
    network: DataHeaderOf<NetworkId>,
}

pub struct PersistEnv {
    pub id: EnvId,
    pub storage_id: StorageId,
    pub network: NetworkId,
    /// List of nodes and their states or external node info
    pub nodes: Vec<(NodeKey, EnvNode)>,
    /// Loaded cannon configs in this env
    pub cannons: Vec<(CannonId, TxSource, TxSink)>,
}

impl From<&Environment> for PersistEnv {
    fn from(value: &Environment) -> Self {
        let nodes = value
            .nodes
            .iter()
            .map(|ent| (ent.key().clone(), ent.value().clone()))
            .collect();

        PersistEnv {
            id: value.id,
            storage_id: value.storage.id,
            network: value.network,
            nodes,
            cannons: value
                .cannons
                .iter()
                .map(|(id, cannon)| (*id, cannon.source.clone(), cannon.sink.clone()))
                .collect(),
        }
    }
}

impl PersistEnv {
    pub async fn load(
        self,
        state: Arc<GlobalState>,
        cannons_ready: Arc<Semaphore>,
    ) -> Result<Environment, EnvError> {
        let storage = state
            .storage
            .get(&(self.network, self.storage_id))
            .ok_or(PrepareError::MissingStorage)?;

        let nodes = self.nodes.into_iter().collect();

        let compute_aot_bin = storage.resolve_compute_binary(&state).await?;

        let (cannons, sinks) = prepare_cannons(
            Arc::clone(&state),
            storage.value(),
            None,
            cannons_ready,
            (self.id, self.network, self.storage_id, compute_aot_bin),
            self.cannons,
        )?;

        // ensure on hydrate that all transactions that were interrupted are
        // marked as authorized
        for (cannon_id, cannon) in &cannons {
            for mut tracker in cannon.transactions.iter_mut() {
                if matches!(tracker.status, TransactionSendState::Executing(_)) {
                    tracker.status = TransactionSendState::Authorized;
                    if let Err(e) = TransactionTracker::write_status(
                        &state,
                        &(self.id, *cannon_id, tracker.key().to_owned()),
                        TransactionSendState::Authorized,
                    ) {
                        tracing::error!(
                            "cannon {}.{cannon_id} failed to write status for {}: {e}",
                            self.id,
                            tracker.key()
                        );
                    }
                }
            }
        }

        Ok(Environment {
            id: self.id,
            network: self.network,
            storage: storage.clone(),
            nodes,
            sinks,
            cannons,
        })
    }
}

impl DataFormat for PersistEnvFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 2;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        written += writer.write_data(&self.version)?;
        written += write_dataformat(writer, &self.nodes)?;
        written += write_dataformat(writer, &self.tx_source)?;
        written += write_dataformat(writer, &self.tx_sink)?;
        written += writer.write_data(&self.network)?;
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header > Self::LATEST_HEADER || *header < 1 {
            return Err(DataReadError::unsupported(
                "PersistEnvHeader",
                format!("1 or {}", Self::LATEST_HEADER),
                header,
            ));
        }

        let version = reader.read_data(&())?;
        let nodes = read_dataformat(reader)?;
        let tx_source = read_dataformat(reader)?;
        let tx_sink = read_dataformat(reader)?;
        let network = if *header > 1 {
            reader.read_data(&())?
        } else {
            0
        };

        Ok(PersistEnvFormatHeader {
            version,
            nodes,
            tx_source,
            tx_sink,
            network,
        })
    }
}

impl DataFormat for PersistEnv {
    type Header = PersistEnvFormatHeader;
    const LATEST_HEADER: Self::Header = PersistEnvFormatHeader {
        version: 1,
        nodes: EnvNode::LATEST_HEADER,
        tx_source: TxSource::LATEST_HEADER,
        tx_sink: TxSink::LATEST_HEADER,
        network: NetworkId::LATEST_HEADER,
    };

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;

        written += writer.write_data(&self.id)?;
        written += writer.write_data(&self.storage_id)?;
        written += writer.write_data(&self.nodes)?;
        written += writer.write_data(&self.cannons)?;
        written += writer.write_data(&self.network)?;

        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(DataReadError::unsupported(
                "PersistEnv",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        let id = reader.read_data(&())?;
        let storage_id = reader.read_data(&())?;
        let nodes = reader.read_data(&(header.tx_source.node_targets, header.nodes.clone()))?;
        let cannons = reader.read_data(&((), header.tx_source.clone(), header.tx_sink.clone()))?;
        let network = if header.network > 0 {
            reader.read_data(&header.network)?
        } else {
            NetworkId::default()
        };

        Ok(PersistEnv {
            id,
            storage_id,
            network,
            nodes,
            cannons,
        })
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use snops_common::{
        format::{read_dataformat, write_dataformat, DataFormat},
        schema::{
            cannon::{sink::TxSink, source::TxSource},
            persist::{TxSinkFormatHeader, TxSourceFormatHeader},
        },
        state::{InternedId, NetworkId},
    };

    use crate::{
        env::EnvNode,
        persist::{EnvNodeStateFormatHeader, PersistEnv, PersistEnvFormatHeader},
    };

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr, $b:expr) => {
            #[test]
            fn $name() -> Result<(), Box<dyn std::error::Error>> {
                let mut data = Vec::new();
                write_dataformat(&mut data, &$a)?;
                assert_eq!(data, $b);

                let mut reader = &data[..];
                let read_value = read_dataformat::<_, $ty>(&mut reader)?;

                // write the data again because not every type implements PartialEq
                let mut data2 = Vec::new();
                write_dataformat(&mut data2, &read_value)?;
                assert_eq!(data, data2);
                Ok(())
            }
        };
    }

    case!(
        env_header,
        PersistEnvFormatHeader,
        PersistEnv::LATEST_HEADER,
        [
            PersistEnvFormatHeader::LATEST_HEADER.to_byte_vec()?,
            PersistEnv::LATEST_HEADER.version.to_byte_vec()?,
            EnvNodeStateFormatHeader::LATEST_HEADER.to_byte_vec()?,
            EnvNode::LATEST_HEADER.to_byte_vec()?,
            TxSourceFormatHeader::LATEST_HEADER.to_byte_vec()?,
            TxSource::LATEST_HEADER.to_byte_vec()?,
            TxSinkFormatHeader::LATEST_HEADER.to_byte_vec()?,
            TxSink::LATEST_HEADER.to_byte_vec()?,
            NetworkId::LATEST_HEADER.to_byte_vec()?,
        ]
        .concat()
    );

    case!(
        env,
        PersistEnv,
        PersistEnv {
            id: InternedId::from_str("foo")?,
            storage_id: InternedId::from_str("bar")?,
            network: Default::default(),
            nodes: Default::default(),
            cannons: Default::default(),
        },
        [
            PersistEnvFormatHeader::LATEST_HEADER.to_byte_vec()?,
            PersistEnv::LATEST_HEADER.to_byte_vec()?,
            InternedId::from_str("foo")?.to_byte_vec()?,
            InternedId::from_str("bar")?.to_byte_vec()?,
            Vec::<(String, EnvNode)>::new().to_byte_vec()?,
            Vec::<(InternedId, TxSource, TxSink)>::new().to_byte_vec()?,
            NetworkId::default().to_byte_vec()?,
        ]
        .concat()
    );
}
