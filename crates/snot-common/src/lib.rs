pub mod rpc;
pub mod state;

pub mod prelude {
    pub use crate::rpc::*;
    pub use crate::state::*;
}

lazy_static::lazy_static! {
    pub static ref INTERN: lasso::ThreadedRodeo = lasso::ThreadedRodeo::default();
}
