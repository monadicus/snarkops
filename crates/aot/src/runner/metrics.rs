// This file exists because the NAMES in the snarkos-node-metrics are
// `pub(super)` for some reason.

use snarkos_node_metrics::{bft, blocks, consensus, router, tcp};

pub const COUNTER_NAMES: [&str; 2] = [
    bft::LEADERS_ELECTED,
    consensus::STALE_UNCONFIRMED_TRANSMISSIONS,
];

pub const GAUGE_NAMES: [&str; 26] = [
    bft::CONNECTED,
    bft::CONNECTING,
    bft::LAST_STORED_ROUND,
    bft::PROPOSAL_ROUND,
    bft::CERTIFIED_BATCHES,
    bft::HEIGHT,
    bft::LAST_COMMITTED_ROUND,
    bft::IS_SYNCED,
    blocks::SOLUTIONS,
    blocks::TRANSACTIONS,
    blocks::ACCEPTED_DEPLOY,
    blocks::ACCEPTED_EXECUTE,
    blocks::REJECTED_DEPLOY,
    blocks::REJECTED_EXECUTE,
    blocks::ABORTED_TRANSACTIONS,
    blocks::ABORTED_SOLUTIONS,
    blocks::PROOF_TARGET,
    blocks::COINBASE_TARGET,
    blocks::CUMULATIVE_PROOF_TARGET,
    consensus::COMMITTED_CERTIFICATES,
    consensus::UNCONFIRMED_SOLUTIONS,
    consensus::UNCONFIRMED_TRANSACTIONS,
    router::CONNECTED,
    router::CANDIDATE,
    router::RESTRICTED,
    tcp::TCP_TASKS,
];

pub const HISTOGRAM_NAMES: [&str; 3] = [
    bft::COMMIT_ROUNDS_LATENCY,
    consensus::CERTIFICATE_COMMIT_LATENCY,
    consensus::BLOCK_LATENCY,
];
