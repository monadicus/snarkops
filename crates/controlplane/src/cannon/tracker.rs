use std::sync::Arc;

use snops_common::{aot_cmds::Authorization, format::PackedUint, state::TransactionSendState};

use super::error::CannonError;
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
        Ok(state.db.tx_index.save(key, &PackedUint(index))?)
    }

    /// Write the transaction number of attempts to the store
    pub fn inc_attempts(state: &GlobalState, key: &TxEntry) -> Result<(), CannonError> {
        // read the previous number of attempts
        let prev = state.db.tx_attempts.restore(key)?.map(|v| v.0).unwrap_or(0);
        Ok(state.db.tx_attempts.save(key, &PackedUint(prev + 1))?)
    }

    /// Get the number of attempts for the transaction
    pub fn get_attempts(state: &GlobalState, key: &TxEntry) -> u32 {
        state
            .db
            .tx_attempts
            .restore(key)
            .ok()
            .flatten()
            .map(|v| v.0 as u32)
            .unwrap_or(0)
    }

    /// Clear the number of attempts for the transaction
    pub fn clear_attempts(state: &GlobalState, key: &TxEntry) -> Result<(), CannonError> {
        state.db.tx_attempts.delete(key)?;
        Ok(())
    }

    /// Write the transaction tracker's status to the store
    pub fn write_status(
        state: &GlobalState,
        key: &TxEntry,
        status: TransactionSendState,
    ) -> Result<(), CannonError> {
        Ok(state.db.tx_status.save(key, &status)?)
    }

    /// Write the transaction tracker's authorization to the store
    pub fn write_auth(
        state: &GlobalState,
        key: &TxEntry,
        auth: &Authorization,
    ) -> Result<(), CannonError> {
        Ok(state.db.tx_auths.save(key, auth)?)
    }

    /// Write the transaction tracker's transaction to the store
    pub fn write_tx(
        state: &GlobalState,
        key: &TxEntry,
        tx: &serde_json::Value,
    ) -> Result<(), CannonError> {
        Ok(state.db.tx_blobs.save(key, tx)?)
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

    /// Remove the transaction tracker from the store
    pub fn delete(state: &GlobalState, key: &TxEntry) -> Result<(), CannonError> {
        state.db.tx_index.delete(key)?;
        state.db.tx_attempts.delete(key)?;
        state.db.tx_status.delete(key)?;
        state.db.tx_auths.delete(key)?;
        state.db.tx_blobs.delete(key)?;
        Ok(())
    }
}
