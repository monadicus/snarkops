/// The environment variable that agents use to authorize with the control
/// plane.
pub const ENV_AGENT_KEY: &str = "SNOPS_AGENT_KEY";
/// The agent key header that is set to [`ENV_AGENT_KEY`].
pub const HEADER_AGENT_KEY: &str = "X-Snops-Agent-Key";
/// The snarkOS binary file name.
pub const SNARKOS_FILE: &str = "snarkos-aot";
/// The snarkOS log file name.
pub const SNARKOS_LOG_FILE: &str = "snarkos.log";
/// The genesis block file name.
pub const SNARKOS_GENESIS_FILE: &str = "genesis.block";
/// The ledger directory name.
pub const LEDGER_BASE_DIR: &str = "ledger";
/// The directory name for persisted ledgers within the storage dir.
pub const LEDGER_PERSIST_DIR: &str = "persist";
/// Temporary storage archive file name.
pub const LEDGER_STORAGE_FILE: &str = "ledger.tar.gz";
/// File containing a version counter for a ledger
pub const VERSION_FILE: &str = "version";
