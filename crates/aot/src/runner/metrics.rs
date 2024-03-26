// This file exists because the NAMES in the snarkos-node-metrics are
// `pub(super)` for some reason.

use metrics::{bft, blocks, consensus, router, tcp};

pub const COUNTER_NAMES: [&str; 1] = [bft::LEADERS_ELECTED];

pub const GAUGE_NAMES: [&str; 18] = [
    bft::CONNECTED,
    bft::CONNECTING,
    bft::LAST_STORED_ROUND,
    bft::PROPOSAL_ROUND,
    bft::CERTIFIED_BATCHES,
    blocks::HEIGHT,
    blocks::SOLUTIONS,
    blocks::TRANSACTIONS,
    blocks::TRANSMISSIONS,
    consensus::COMMITTED_CERTIFICATES,
    consensus::LAST_COMMITTED_ROUND,
    consensus::UNCONFIRMED_SOLUTIONS,
    consensus::UNCONFIRMED_TRANSACTIONS,
    consensus::UNCONFIRMED_TRANSMISSIONS,
    router::CONNECTED,
    router::CANDIDATE,
    router::RESTRICTED,
    tcp::TCP_TASKS,
];

pub const HISTOGRAM_NAMES: [&str; 7] = [
    bft::COMMIT_ROUNDS_LATENCY,
    consensus::CERTIFICATE_COMMIT_LATENCY,
    consensus::BLOCK_LATENCY,
    tcp::NOISE_CODEC_ENCRYPTION_TIME,
    tcp::NOISE_CODEC_DECRYPTION_TIME,
    tcp::NOISE_CODEC_ENCRYPTION_SIZE,
    tcp::NOISE_CODEC_DECRYPTION_SIZE,
];
