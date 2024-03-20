use std::sync::{Arc, Mutex};

pub type AppState = Arc<GlobalState>;

/// Global state for this agent runner.
pub struct GlobalState {
    pub jwt: Mutex<Option<String>>,
    // TODO: include snarkOS process handler/channels/something
}
