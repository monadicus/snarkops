use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use chrono::{TimeDelta, Utc};
use dashmap::{mapref::entry::Entry, DashMap};
use snops_common::{
    rpc::control::ControlServiceClient,
    state::{TransferId, TransferStatus, TransferStatusUpdate},
};
use tarpc::context;
use tokio::{select, sync::mpsc};

pub type TransferTx = mpsc::UnboundedSender<(TransferId, TransferStatusUpdate)>;

// how long to wait before cleaning up a transfer that has ended
pub const TRANSFER_CLEANUP_DELAY_OK: TimeDelta = TimeDelta::seconds(60);
pub const TRANSFER_CLEANUP_DELAY_ERR: TimeDelta = TimeDelta::seconds(60 * 60 * 2);

pub fn next_id() -> TransferId {
    static TRANSFER_ID_CTR: AtomicU32 = AtomicU32::new(0);
    TRANSFER_ID_CTR.fetch_add(1, Ordering::AcqRel)
}

pub fn start_monitor(
    client: ControlServiceClient,
) -> (TransferTx, Arc<DashMap<TransferId, TransferStatus>>) {
    let (tx, mut rx) = mpsc::unbounded_channel::<(TransferId, TransferStatusUpdate)>();
    let state_transfers = Arc::new(DashMap::new());

    let transfers = Arc::clone(&state_transfers);
    tokio::spawn(async move {
        use TransferStatusUpdate::*;
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

        loop {
            select! {
                // cleanup transfers that have ended
                _ = interval.tick() => {
                    let now = Utc::now();
                    let client = client.clone();
                    transfers.retain(|&id, transfer: &mut TransferStatus| {
                        let is_done = transfer.total_bytes == transfer.downloaded_bytes;
                        let is_error = transfer.interruption.is_some();

                        // incomplete transfers are retained
                        if !is_done && !is_error {
                            return true
                        }

                        // cleanup transfers that have ended and have been around for a while
                        let cleanup_delay = if is_error { TRANSFER_CLEANUP_DELAY_ERR } else { TRANSFER_CLEANUP_DELAY_OK };
                        let keep = now.signed_duration_since(transfer.updated_at) < cleanup_delay;

                        if !keep {
                            // send the update to the control plane
                            let client = client.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client.post_transfer_status(context::current(), id, TransferStatusUpdate::Cleanup).await {
                                    tracing::error!("failed to send transfer cleanup update: {e}");
                                }
                            });
                        }

                        keep
                    });
                }

                // handle incoming messages and update the transfers map
                Some((id, message)) = rx.recv() => {
                    match (message.clone(), transfers.entry(id)) {
                        // insert new transfer
                        (Start { time, desc, total }, Entry::Vacant(ent)) => {
                            ent.insert(TransferStatus {
                                started_at: time,
                                updated_at: time,
                                desc,
                                total_bytes: total,
                                downloaded_bytes: 0,
                                interruption: None,
                            });
                        },

                        // update progress of an existing transfer
                        (Progress { downloaded }, Entry::Occupied(mut ent)) => {
                            let transfer = ent.get_mut();
                            transfer.downloaded_bytes = downloaded;
                            transfer.updated_at = Utc::now();
                        },

                        // end a transfer
                        (End { interruption }, Entry::Occupied(mut ent)) => {
                            let transfer = ent.get_mut();
                            if interruption.is_none() {
                                transfer.downloaded_bytes = transfer.total_bytes;
                            }
                            transfer.interruption = interruption;
                            transfer.updated_at = Utc::now();
                        },

                        _ => continue,
                    }

                    // send the update to the control plane
                    let client = client.clone();
                    tokio::spawn(async move {
                        if let Err(e) = client.post_transfer_status(context::current(), id, message).await {
                            tracing::error!("failed to send transfer status update: {e}");
                        }
                    });
                }
            }
        }
    });

    (tx, state_transfers)
}
