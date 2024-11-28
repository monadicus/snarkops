pub mod agent;
pub mod command;
mod files;
pub use files::*;
use snops_common::state::ReconcileStatus;
pub mod address;
pub mod process;
pub mod state;
pub mod storage;

pub trait Reconcile<T, E> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<T>, E>;
}
