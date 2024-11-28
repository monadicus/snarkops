use std::sync::Arc;

use futures_util::Stream;
use tokio::sync::broadcast::{self, error::TryRecvError};

use super::{Event, EventFilter};

#[derive(Debug)]
pub struct Events {
    tx: broadcast::Sender<Arc<Event>>,
}

impl Events {
    pub fn new() -> Self {
        Self {
            tx: broadcast::channel(1024).0,
        }
    }

    pub fn emit(&self, event: Event) {
        if self.tx.receiver_count() == 0 {
            return;
        }
        // The only way this can fail is a receiver was dropped between the above check
        // and this call...
        let _ = self.tx.send(Arc::new(event));
    }

    pub fn subscribe(&self) -> EventSubscriber {
        EventSubscriber {
            rx: self.tx.subscribe(),
            filter: EventFilter::Unfiltered,
        }
    }

    pub fn subscribe_on(&self, filter: EventFilter) -> EventSubscriber {
        EventSubscriber {
            rx: self.tx.subscribe(),
            filter,
        }
    }
}

impl Default for Events {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EventSubscriber {
    rx: broadcast::Receiver<Arc<Event>>,
    filter: EventFilter,
}

impl EventSubscriber {
    pub async fn next(&mut self) -> Result<Arc<Event>, broadcast::error::RecvError> {
        self.rx.recv().await
    }
}

impl Stream for EventSubscriber {
    type Item = Arc<Event>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        loop {
            match self.rx.try_recv() {
                Ok(event) if event.matches(&self.filter) => {
                    return std::task::Poll::Ready(Some(event));
                }
                // skip events that don't match the filter
                Ok(_) => continue,
                Err(TryRecvError::Closed) => {
                    return std::task::Poll::Ready(None);
                }
                Err(TryRecvError::Empty) => {
                    return std::task::Poll::Pending;
                }
                Err(TryRecvError::Lagged(n)) => {
                    tracing::warn!("{n} events dropped by a subscriber");
                }
            }
        }
    }
}
