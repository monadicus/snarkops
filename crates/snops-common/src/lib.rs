pub mod action_models;
#[cfg(feature = "aot_cmds")]
pub mod aot_cmds;
pub mod rpc;
pub mod set;
pub mod state;
pub use lasso;
pub mod api;
pub mod constant;
pub mod format;
pub mod key_source;
pub mod node_targets;

#[cfg(feature = "clipages")]
pub mod clipages;
#[cfg(feature = "mangen")]
pub mod mangen;

pub mod prelude {
    pub use crate::rpc::*;
    pub use crate::set::*;
    pub use crate::state::*;
}

lazy_static::lazy_static! {
    pub static ref INTERN: lasso::ThreadedRodeo = lasso::ThreadedRodeo::default();
}
