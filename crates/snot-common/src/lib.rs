pub mod rpc;
pub mod set;
pub mod state;
pub use lasso;
pub mod api;

pub mod prelude {
    pub use crate::rpc::*;
    pub use crate::set::*;
    pub use crate::state::*;
}

lazy_static::lazy_static! {
    pub static ref INTERN: lasso::ThreadedRodeo = lasso::ThreadedRodeo::default();
}
