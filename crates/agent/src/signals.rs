use futures::{StreamExt, stream::FuturesUnordered};
use tokio::signal::unix::{Signal, SignalKind, signal};

pub struct Signals {
    signals: Vec<Signal>,
}

impl Signals {
    pub fn new(kinds: &[SignalKind]) -> Self {
        Self {
            signals: kinds.iter().map(|k| signal(*k).unwrap()).collect(),
        }
    }

    pub fn term_or_interrupt() -> Self {
        Self::new(&[SignalKind::terminate(), SignalKind::interrupt()])
    }

    pub async fn recv_any(&mut self) {
        let mut futs = FuturesUnordered::new();

        for sig in self.signals.iter_mut() {
            futs.push(sig.recv());
        }

        futs.next().await;
    }
}

pub fn term_or_interrupt() -> Signals {
    Signals::term_or_interrupt()
}
