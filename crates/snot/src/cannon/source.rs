use std::collections::HashSet;

use anyhow::{anyhow, Result};
use serde::Deserialize;
use snot_common::state::NodeKey;

use super::net::get_available_port;

/// Represents an instance of a local query service.
#[derive(Debug, Deserialize)]
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
    /// requires cannon to have an associated test_id
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
#[derive(Debug, Deserialize)]
pub enum LedgerQueryService {
    /// Use the local ledger query service
    Local(LocalQueryService),
    /// Target a specific node (probably over rpc instead of reqwest lol...)
    ///
    /// Requires cannon to have an associated test_id
    Node(NodeKey),
}

impl LedgerQueryService {
    pub fn needs_test_id(&self) -> bool {
        match self {
            LedgerQueryService::Node(_) => true,
            LedgerQueryService::Local(LocalQueryService { sync_from, .. }) => sync_from.is_some(),
        }
    }
}

/// Which service is providing the compute power for executing transactions
#[derive(Debug, Deserialize)]
pub enum ComputeTarget {
    /// Use the agent pool to generate executions
    AgentPool,
    /// Use demox' API to generate executions
    Demox,
}

#[derive(Debug, Hash, PartialEq, Eq, Deserialize)]
pub enum CreditsTxMode {
    BondPublic,
    UnbondPublic,
    TransferPublic,
    TransferPublicToPrivate,
    // cannot run these in aot mode
    TransferPrivate,
    TransferPrivateToPublic,
}

#[derive(Debug, Hash, PartialEq, Eq, Deserialize)]
pub enum TxMode {
    Credits(CreditsTxMode),
    // TODO: Program(program, func, input types??)
}

#[derive(Debug, Deserialize)]
pub enum TxSource {
    /// Read transactions from a file
    AoTPlayback {
        // filename from the storage for the tx list
        name: String,
    },
    /// Generate transactions in real time
    RealTime {
        query: LedgerQueryService,
        compute: ComputeTarget,

        tx_modes: HashSet<TxMode>,

        /// how many transactions to buffer before firing a burst
        min_buffer_size: usize,
    },
}

impl TxSource {
    pub fn needs_test_id(&self) -> bool {
        match self {
            TxSource::RealTime { query, .. } => query.needs_test_id(),
            _ => false,
        }
    }

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
}
