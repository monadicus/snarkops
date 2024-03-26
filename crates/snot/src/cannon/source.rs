use std::collections::HashSet;

use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use snot_common::state::NodeKey;

use crate::{env::Environment, schema::nodes::KeySource};

use super::{authorized::Authorize, net::get_available_port};

/// Represents an instance of a local query service.
#[derive(Clone, Debug, Deserialize)]
pub struct LocalQueryService {
    /// Ledger & genesis block to use
    // pub storage_id: usize,
    /// port to host the service on (needs to be unused by other cannons and services)
    /// this port will be use when forwarding requests to the local query service
    // pub port: u16,

    // TODO debate this
    /// An optional node to sync blocks from...
    /// necessary for private tx mode in realtime mode as this will have to
    /// sync from a node that has a valid ledger
    ///
    /// When present, the cannon will update the ledger service from this node
    /// if the node is out of sync, it will corrupt the ledger...
    ///
    /// requires cannon to have an associated env_id
    pub sync_from: Option<NodeKey>,
}

impl LocalQueryService {
    // TODO: cache this when sync_from is false
    /// Fetch the state root from the local query service
    /// (non-cached)
    pub async fn get_state_root(&self, port: u16) -> Result<String> {
        let url = format!("http://127.0.0.1:{}/mainnet/latest/stateRoot", port);
        let response = reqwest::get(&url).await?;
        Ok(response.text().await?)
    }
}

/// Used to determine the redirection for the following paths:
/// /cannon/<id>/mainnet/latest/stateRoot
/// /cannon/<id>/mainnet/transaction/broadcast
#[derive(Clone, Debug, Deserialize)]
pub enum LedgerQueryService {
    /// Use the local ledger query service
    Local(LocalQueryService),
    /// Target a specific node (probably over rpc instead of reqwest lol...)
    ///
    /// Requires cannon to have an associated env_id
    Node(NodeKey),
}

/// Which service is providing the compute power for executing transactions
#[derive(Default, Clone, Debug, Deserialize)]
pub enum ComputeTarget {
    /// Use the agent pool to generate executions
    #[default]
    AgentPool,
    /// Use demox' API to generate executions
    Demox { url: String },
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Deserialize)]
pub enum CreditsTxMode {
    BondPublic,
    UnbondPublic,
    TransferPublic,
    TransferPublicToPrivate,
    // cannot run these in aot mode
    TransferPrivate,
    TransferPrivateToPublic,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Deserialize)]
pub enum TxMode {
    Credits(CreditsTxMode),
    // TODO: Program(program, func, input types??)
}

#[derive(Clone, Debug, Deserialize)]
pub enum TxSource {
    /// Read transactions from a file
    Playback {
        // filename from the storage for the tx list
        name: String,
    },
    /// Generate transactions in real time
    RealTime {
        query: LedgerQueryService,
        compute: ComputeTarget,

        /// defaults to TransferPublic
        tx_modes: HashSet<TxMode>,

        /// private keys for making transactions
        /// defaults to committee keys
        private_keys: Vec<KeySource>,
        /// addreses for transaction targets
        /// defaults to committee addresses
        addresses: Vec<KeySource>,
    },
}

impl TxSource {
    /// Get an available port for the query service if applicable
    pub fn get_query_port(&self) -> Result<Option<u16>> {
        matches!(
            self,
            TxSource::RealTime {
                query: LedgerQueryService::Local(_),
                ..
            }
        )
        .then(|| get_available_port().ok_or(anyhow!("could not get an available port")))
        .transpose()
    }

    pub fn get_auth(&self, env: &Environment) -> Result<Authorize> {
        match self {
            TxSource::RealTime {
                tx_modes,
                private_keys,
                addresses,
                ..
            } => {
                let sample_pk = || {
                    private_keys
                        .get(rand::random::<usize>() % private_keys.len())
                        .and_then(|k| env.storage.sample_keysource_pk(k))
                        .ok_or(anyhow!("error selecting a valid private key"))
                };
                let sample_addr = || {
                    addresses
                        .get(rand::random::<usize>() % addresses.len())
                        .and_then(|k| env.storage.sample_keysource_addr(k))
                        .ok_or(anyhow!("error selecting a valid private key"))
                };

                let Some(mode) = tx_modes
                    .iter()
                    .nth(rand::random::<usize>() % tx_modes.len())
                else {
                    bail!("no tx modes available for this cannon instance??")
                };

                let auth = match mode {
                    TxMode::Credits(credit) => match credit {
                        CreditsTxMode::BondPublic => todo!(),
                        CreditsTxMode::UnbondPublic => todo!(),
                        CreditsTxMode::TransferPublic => Authorize::TransferPublic {
                            private_key: sample_pk()?,
                            recipient: sample_addr()?,
                            amount: 1,
                            priority_fee: 0,
                        },
                        CreditsTxMode::TransferPublicToPrivate => todo!(),
                        CreditsTxMode::TransferPrivate => todo!(),
                        CreditsTxMode::TransferPrivateToPublic => todo!(),
                    },
                };

                Ok(auth)
            }
            _ => Err(anyhow!("cannot authorize playback transactions")),
        }
    }
}
