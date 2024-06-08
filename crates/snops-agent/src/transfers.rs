use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use dashmap::{mapref::entry::Entry, DashMap};
use tokio::sync::mpsc;

pub type TransferId = usize;
pub type TransferInterruption = ();
pub type TransferTx = mpsc::UnboundedSender<TransferMessage>;

pub enum TransferMessage {
    Start {
        id: TransferId,
        desc: Option<String>,
        /// The number of bytes expected to transfer.
        bytes: usize,
    },
    Progress {
        id: TransferId,
        /// The current number of bytes transferred.
        bytes: usize,
    },
    End {
        id: TransferId,
        /// An interruption reason, if any.
        interruption: Option<TransferInterruption>,
    },
}

#[derive(Debug, Clone)]
pub struct Transfer {
    pub desc: Option<String>,
    pub total: usize,
    pub state: TransferState,
}

#[derive(Debug, Clone)]
pub enum TransferState {
    Active(usize),
    Ended {
        interruption: Option<TransferInterruption>,
    },
}

pub fn next_id() -> TransferId {
    static TRANSFER_ID_CTR: AtomicUsize = AtomicUsize::new(0);
    TRANSFER_ID_CTR.fetch_add(1, Ordering::AcqRel)
}

pub fn start_monitor() -> (TransferTx, Arc<DashMap<TransferId, Transfer>>) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let state_transfers = Arc::new(DashMap::new());

    let transfers = Arc::clone(&state_transfers);
    tokio::spawn(async move {
        use TransferMessage::*;
        while let Some(message) = rx.recv().await {
            match message {
                Start { id, desc, bytes } => match transfers.entry(id) {
                    Entry::Occupied(_) => continue,
                    Entry::Vacant(ent) => {
                        ent.insert(Transfer {
                            desc,
                            total: bytes,
                            state: TransferState::Active(0),
                        });
                    }
                },

                Progress { id, bytes } => match transfers.entry(id) {
                    Entry::Occupied(mut ent) => match ent.get_mut().state {
                        TransferState::Active(ref mut cur) => *cur = bytes,
                        TransferState::Ended { .. } => continue,
                    },
                    Entry::Vacant(_) => continue,
                },

                End { id, interruption } => match transfers.entry(id) {
                    Entry::Occupied(mut ent) => {
                        ent.get_mut().state = TransferState::Ended { interruption };
                    }
                    Entry::Vacant(_) => continue,
                },
            }
        }
    });

    (tx, state_transfers)
}
