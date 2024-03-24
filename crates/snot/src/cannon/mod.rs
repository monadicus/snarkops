use std::collections::{HashSet, VecDeque};

use snot_common::state::NodeKey;
use tokio::process::Child;

use crate::schema::NodeTargets;

/*

STEP ONE
cannon transaction source: (GEN OR PLAYBACK)
- AOT: storage file
- REALTIME: generate executions from available agents?? via rpc


STEP 2
cannon query source:
/cannon/<id>/mainnet/latest/stateRoot forwards to one of the following:
- REALTIME-(GEN|PLAYBACK): (test_id, node-key) with a rest ports Client/Validator only
- AOT-GEN: ledger service locally (file mode)
- AOT-PLAYBACK: n/a

STEP 3
cannon broadcast ALWAYS HITS control plane at
/cannon/<id>/mainnet/transaction/broadcast
cannon TX OUTPUT pointing at
- REALTIME: (test_id, node-key)
- AOT: file


cannon rate
cannon buffer size
burst mode??

*/

/// Represents an instance of a local query service.
#[derive(Debug)]
struct LocalQueryService {
    /// child process running the ledger query service
    child: Child,
    /// Ledger & genesis block to use
    pub storage_id: usize,
    /// port to host the service on (needs to be unused by other cannons and services)
    /// this port will be use when forwarding requests to the local query service
    pub port: u16,

    // TODO debate this
    /// An optional node to sync blocks from...
    /// necessary for private tx mode in realtime mode as this will have to
    /// sync from a node that has a valid ledger
    ///
    /// When present, the cannon will update the ledger service from this node
    /// if the node is out of sync, it will corrupt the ledger...
    pub sync_from: Option<(NodeKey, usize)>,
}

/// Used to determine the redirection for the following paths:
/// /cannon/<id>/mainnet/latest/stateRoot
/// /cannon/<id>/mainnet/transaction/broadcast
#[derive(Debug)]
enum LedgerQueryService {
    /// Use the local ledger query service
    Local(LocalQueryService),
    /// Target a specific node (probably over rpc instead of reqwest lol...)
    Node { target: NodeKey, test_id: usize },
}

/// Which service is providing the compute power for executing transactions
#[derive(Debug)]
enum ComputeTarget {
    /// Use the agent pool to generate executions
    AgentPool,
    /// Use demox' API to generate executions
    Demox,
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub enum CreditsTxMode {
    BondPublic,
    UnbondPublic,
    TransferPublic,
    TransferPublicToPrivate,
    // cannot run these in aot mode
    TransferPrivate,
    TransferPrivateToPublic,
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub enum TxMode {
    Credits(CreditsTxMode),
    // TODO: Program(program, func, input types??)
}

#[derive(Debug)]
enum TxSource {
    /// Read transactions from a file
    AoT {
        storage_id: usize,
        // filename for the tx list
        name: String,
    },
    /// Generate transactions in real time
    RealTime {
        query: LedgerQueryService,
        compute: ComputeTarget,

        tx_modes: HashSet<TxMode>,

        /// buffer of transactions to send
        tx_buffer: VecDeque<String>,

        /// how many transactions to buffer before firing a burst
        min_buffer_size: usize,
    },
}

#[derive(Debug)]
enum TxSink {
    /// Write transactions to a file
    AoT {
        storage_id: usize,
        /// filename for the recording txs list
        name: String,
    },
    /// Send transactions to nodes in a test
    RealTime {
        target: NodeTargets,
        test_id: usize,

        /// How long between each burst of transactions
        burst_delay_ms: u32,
        /// How many transactions to fire off in each burst
        tx_per_burst: u32,
        /// How long between each transaction in a burst
        tx_delay_ms: u32,
    },
}

/// Transaction cannon
#[derive(Debug)]
pub struct TestCannon {
    source: TxSource,
    sink: TxSink,
}
