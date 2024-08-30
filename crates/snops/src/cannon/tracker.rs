use std::sync::Arc;

use snops_common::aot_cmds::Authorization;

use super::{error::CannonError, status::TransactionSendState};
use crate::{db::TxEntry, state::GlobalState};

#[derive(Debug, Clone)]
pub struct TransactionTracker {
    /// Index of the transaction, used for ordering
    pub index: u64,
    /// Optional transaction authorization. Must be present if transaction
    /// is None.
    pub authorization: Option<Arc<Authorization>>,
    /// JSON for the actual transaction. If not present, will be
    /// generated from the authorization.
    pub transaction: Option<Arc<serde_json::Value>>,
    /// Status of the transaction
    pub status: TransactionSendState,
}

impl TransactionTracker {
    /// Write the transaction tracker's index to the store
    pub fn write_index(state: &GlobalState, key: &TxEntry, index: u64) -> Result<(), CannonError> {
        state
            .db
            .tx_index
            .save(key, &index)
            .map_err(|e| CannonError::DatabaseWriteError(format!("transaction index {}", key.2), e))
    }

    /// Write the transaction tracker's status to the store
    pub fn write_status(
        state: &GlobalState,
        key: &TxEntry,
        status: TransactionSendState,
    ) -> Result<(), CannonError> {
        state.db.tx_status.save(key, &status).map_err(|e| {
            CannonError::DatabaseWriteError(format!("transaction status {}", key.2), e)
        })
    }

    /// Write the transaction tracker's authorization to the store
    pub fn write_auth(
        state: &GlobalState,
        key: &TxEntry,
        auth: &Authorization,
    ) -> Result<(), CannonError> {
        state
            .db
            .tx_auths
            .save(key, auth)
            .map_err(|e| CannonError::DatabaseWriteError(format!("transaction auth {}", key.2), e))
    }

    /// Write the transaction tracker's transaction to the store
    pub fn write_tx(
        state: &GlobalState,
        key: &TxEntry,
        tx: &serde_json::Value,
    ) -> Result<(), CannonError> {
        state
            .db
            .tx_blobs
            .save(key, tx)
            .map_err(|e| CannonError::DatabaseWriteError(format!("transaction blob {}", key.2), e))
    }

    /// Write the transaction tracker to the store
    pub fn write(&self, state: &GlobalState, key: &TxEntry) -> Result<(), CannonError> {
        Self::write_index(state, key, self.index)?;
        Self::write_status(state, key, self.status)?;
        if let Some(auth) = self.authorization.as_deref() {
            Self::write_auth(state, key, auth)?;
        }
        if let Some(tx) = self.transaction.as_deref() {
            Self::write_tx(state, key, tx)?;
        }
        Ok(())
    }
}
