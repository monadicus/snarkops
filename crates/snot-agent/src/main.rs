mod agent;
mod cli;
mod rpc;

use std::{env, time::Duration};

use clap::Parser;
use cli::{Cli, ENV_ENDPOINT, ENV_ENDPOINT_DEFAULT};
use futures::SinkExt;
use futures_util::stream::{FuturesUnordered, StreamExt};
use snot_common::rpc::{AgentService, RpcTransport};
use tarpc::server::Channel;
use tokio::{
    select,
    signal::unix::{signal, Signal, SignalKind},
};
use tokio_tungstenite::{connect_async, tungstenite, tungstenite::http::Uri};
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::rpc::AgentRpcServer;

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

    let args = Cli::parse();

    // create rpc channels
    let (server_request_in, server_transport, mut server_response_out) = RpcTransport::new();

    // initialize and start the rpc server
    let rpc_server = tarpc::server::BaseChannel::with_defaults(server_transport);
    tokio::spawn(
        rpc_server
            .execute(AgentRpcServer.serve())
            .for_each(|r| async move {
                tokio::spawn(r);
            }),
    );

    // TODO(rpc): in order for RPC servers to work in *both* directions, we will
    // need to multiplex the messages sent by tarpc with a wrapping type so that we
    // can properly deserialize them and direct them to the right channels (as in,
    // directing requests from the control plane [agent is server] to the right
    // channel versus responses from the control plane [control plane is server])

    // get the WS endpoint
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

    // get the interrupt signals to break the stream connection
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
                select! {
                    // terminate if an interrupt was triggered
                    _ = interrupt.recv_any() => {
                        terminating = true;
                        break 'event;
                    }

                    // handle outgoing responses
                    msg = server_response_out.recv() => {
                        let msg = msg.expect("internal RPC channel closed");
                        let bin = bincode::serialize(&msg).expect("failed to serialize response");
                        if let Err(_) = ws_stream.send(tungstenite::Message::Binary(bin)).await {
                            error!("The connection to the control plane was interrupted");
                            break 'event;
                        }
                    }

                    // handle incoming messages
                    msg = ws_stream.next() => match msg {
                        Some(Ok(tungstenite::Message::Binary(bin))) => {
                            info!("got a binary message in!");
                            let msg = bincode::deserialize(&bin).expect("deserialize"); // TODO: don't panic
                            server_request_in.send(msg).expect("internal RPC channel closed");
                        }

                        None | Some(Err(_)) => {
                            error!("The connection to the control plane was interrupted");
                            break 'event;
                        }

                        Some(Ok(o)) => {
                            println!("{o:#?}");
                        }
                    },
                };
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
