mod cli;
mod rpc;
mod state;

use std::{
    env,
    sync::{Arc, Mutex},
    time::Duration,
};

use clap::Parser;
use cli::{Cli, ENV_ENDPOINT, ENV_ENDPOINT_DEFAULT};
use futures::SinkExt;
use futures_util::stream::{FuturesUnordered, StreamExt};
use http::HeaderValue;
use snot_common::rpc::{AgentService, ControlServiceClient, RpcTransport};
use tarpc::server::Channel;
use tokio::{
    select,
    signal::unix::{signal, Signal, SignalKind},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, client::IntoClientRequest, http::Uri},
};
use tracing::{error, info, level_filters::LevelFilter, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::rpc::{AgentRpcServer, MuxedMessageIncoming, MuxedMessageOutgoing};
use crate::state::GlobalState;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .parse_lossy("")
                .add_directive("tarpc::client=ERROR".parse().unwrap())
                .add_directive("tarpc::server=ERROR".parse().unwrap()),
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .unwrap();

    let args = Cli::parse();

    // get the JWT from the file, if possible
    // TODO: change this file path
    let jwt = tokio::fs::read_to_string("./jwt.txt").await.ok();

    // create the client state
    let state = Arc::new(GlobalState {
        jwt: Mutex::new(jwt),
    });

    // create rpc channels
    let (client_response_in, client_transport, mut client_request_out) = RpcTransport::new();
    let (server_request_in, server_transport, mut server_response_out) = RpcTransport::new();

    // set up the client, facing the control plane
    let _client =
        ControlServiceClient::new(tarpc::client::Config::default(), client_transport).spawn();

    // initialize and start the rpc server
    let rpc_server = tarpc::server::BaseChannel::with_defaults(server_transport);
    tokio::spawn(
        rpc_server
            .execute(
                AgentRpcServer {
                    state: state.to_owned(),
                }
                .serve(),
            )
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
            let mut req = ws_uri.to_owned().into_client_request().unwrap();

            // attach JWT if we have one
            {
                let jwt = state.jwt.lock().expect("failed to acquire jwt");
                if let Some(jwt) = jwt.as_deref() {
                    req.headers_mut().insert(
                        "Authorization",
                        HeaderValue::from_bytes(format!("Bearer {jwt}").as_bytes())
                            .expect("attach authorization header"),
                    );
                }
            }

            let (mut ws_stream, _) = select! {
                _ = interrupt.recv_any() => break 'process,

                res = connect_async(req) => match res {
                    Ok(c) => c,
                    Err(e) => {
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
                        let bin = bincode::serialize(&MuxedMessageOutgoing::Agent(msg)).expect("failed to serialize response");
                        if let Err(_) = ws_stream.send(tungstenite::Message::Binary(bin)).await {
                            error!("The connection to the control plane was interrupted");
                            break 'event;
                        }
                    }

                    // handle outgoing requests
                    msg = client_request_out.recv() => {
                        let msg = msg.expect("internal RPC channel closed");
                        let bin = bincode::serialize(&MuxedMessageOutgoing::Control(msg)).expect("failed to serialize request");
                        if let Err(_) = ws_stream.send(tungstenite::Message::Binary(bin)).await {
                            error!("The connection to the control plane was interrupted");
                            break 'event;
                        }
                    }

                    // handle incoming messages
                    msg = ws_stream.next() => match msg {
                        Some(Ok(tungstenite::Message::Binary(bin))) => {
                            let Ok(msg) = bincode::deserialize(&bin) else {
                                warn!("failed to deserialize a message from the control plane");
                                continue;
                            };

                            match msg {
                                MuxedMessageIncoming::Agent(msg) => server_request_in.send(msg).expect("internal RPC channel closed"),
                                MuxedMessageIncoming::Control(msg) => client_response_in.send(msg).expect("internal RPC channel closed"),
                            }
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
