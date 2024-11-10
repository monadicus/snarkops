use snops_common::impl_into_status_code;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("batch reconciliation failed with `{failures}` failed reconciliations")]
pub struct BatchReconcileError {
    pub failures: usize,
}

impl_into_status_code!(BatchReconcileError);
