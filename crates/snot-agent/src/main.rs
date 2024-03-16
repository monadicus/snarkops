mod agent;
mod cli;

use std::{env, time::Duration};

use clap::Parser;
use cli::{Cli, ENV_ENDPOINT, ENV_ENDPOINT_DEFAULT};
use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::{
    select,
    signal::unix::{signal, Signal, SignalKind},
};
use tokio_tungstenite::{connect_async, tungstenite::http::Uri};
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .parse_lossy(""),
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .unwrap();

    // TODO: clap args to specify where control plane is
    // TODO: TLS
    let args = Cli::parse();

    let endpoint = args
        .endpoint
        .or_else(|| {
            env::var(ENV_ENDPOINT)
                .ok()
                .and_then(|s| s.as_str().parse().ok())
        })
        .unwrap_or(ENV_ENDPOINT_DEFAULT);

    let ws_uri = Uri::builder()
        .scheme("ws")
        .authority(endpoint.to_string())
        .path_and_query("/agent")
        .build()
        .unwrap();

    let mut interrupt = Signals::new(&[SignalKind::terminate(), SignalKind::interrupt()]);

    'process: loop {
        'connection: {
            let (mut ws_stream, _) = select! {
                _ = interrupt.recv_any() => break 'process,

                res = connect_async(&ws_uri) => match res {
                    Ok(c) => c,
                    Err(e) => {
                        // TODO: print error
                        error!("An error occurred establishing the connection: {e}");
                        break 'connection;
                    },
                },
            };

            info!("Connection established with the control plane");

            let mut terminating = false;

            'event: loop {
                let msg = select! {
                    _ = interrupt.recv_any() => {
                        terminating = true;
                        break 'event;
                    }

                    msg = ws_stream.next() => match msg {
                        Some(Ok(msg)) => msg,
                        _ => {
                            error!("The connection to the control plane was interrupted");
                            break 'event;
                        }
                    },
                };

                // TODO: do something with msg
            }

            if terminating {
                break 'process;
            }
        }

        // wait some time before attempting to reconnect
        select! {
            _ = interrupt.recv_any() => break,

            // TODO: dynamic time
            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                info!("Attempting to reconnect...");
            },
        }
    }

    info!("snot agent has shut down gracefully :)");
}

struct Signals {
    signals: Vec<Signal>,
}

impl Signals {
    fn new(kinds: &[SignalKind]) -> Self {
        Self {
            signals: kinds.iter().map(|k| signal(*k).unwrap()).collect(),
        }
    }

    async fn recv_any(&mut self) {
        let mut futs = FuturesUnordered::new();

        for sig in self.signals.iter_mut() {
            futs.push(sig.recv());
        }

        futs.next().await;
    }
}
