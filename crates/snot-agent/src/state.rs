use std::sync::{Arc, Mutex};

use snot_common::state::AgentState;
use tokio::{
    process::Child,
    sync::{Mutex as AsyncMutex, RwLock},
    task::AbortHandle,
};

use crate::cli::Cli;

pub type AppState = Arc<GlobalState>;

/// Global state for this agent runner.
pub struct GlobalState {
    pub cli: Cli,
    pub jwt: Mutex<Option<String>>,
    pub agent_state: RwLock<AgentState>,
    pub reconcilation_handle: AsyncMutex<Option<AbortHandle>>,
    pub child: RwLock<Option<Child>>, /* TODO: this may need to be handled by an owning thread,
                                       * not sure yet */
}
