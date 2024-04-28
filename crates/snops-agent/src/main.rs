mod api;
mod cli;
mod metrics;
mod net;
mod reconcile;
mod rpc;
mod state;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use clap::Parser;
use cli::Cli;
use futures::SinkExt;
use futures_util::stream::{FuturesUnordered, StreamExt};
use http::HeaderValue;
use snops_common::{
    constant::{ENV_AGENT_KEY, HEADER_AGENT_KEY},
    rpc::{agent::AgentService, control::ControlServiceClient, RpcTransport},
};
use tarpc::server::Channel;
use tokio::{
    select,
    signal::unix::{signal, Signal, SignalKind},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, client::IntoClientRequest},
};
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::rpc::{AgentRpcServer, MuxedMessageIncoming, MuxedMessageOutgoing, JWT_FILE};
use crate::state::GlobalState;

#[tokio::main]
async fn main() {
    let (stdout, _guard) = tracing_appender::non_blocking(std::io::stdout());

    let output: tracing_subscriber::fmt::Layer<
        _,
        tracing_subscriber::fmt::format::DefaultFields,
        tracing_subscriber::fmt::format::Format,
        tracing_appender::non_blocking::NonBlocking,
    > = tracing_subscriber::fmt::layer().with_writer(stdout);

    let output = if cfg!(debug_assertions) {
        output.with_file(true).with_line_number(true)
    } else {
        output
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_env_var("SNOPS_AGENT_LOG")
                .with_default_directive(LevelFilter::TRACE.into())
                .from_env_lossy()
                .add_directive("neli=off".parse().unwrap())
                .add_directive("hyper_util=off".parse().unwrap())
                .add_directive("reqwest=off".parse().unwrap())
                .add_directive("tungstenite=off".parse().unwrap())
                .add_directive("tokio_tungstenite=off".parse().unwrap())
                .add_directive("tarpc::client=ERROR".parse().unwrap())
                .add_directive("tarpc::server=ERROR".parse().unwrap()),
        )
        .with(output)
        .try_init()
        .unwrap();

    let args = Cli::parse();

    // get the network interfaces available to this node
    let internal_addrs = net::get_internal_addrs().expect("failed to get network interfaces");
    let external_addr = args.external;
    if let Some(addr) = external_addr {
        info!("using external addr: {}", addr);
    } else {
        info!("skipping external addr");
    }

    // get the endpoint
    let (endpoint, ws_uri) = args.endpoint_and_uri();
    info!("connecting to {endpoint}");

    // create the data directory
    tokio::fs::create_dir_all(&args.path)
        .await
        .expect("failed to create data path");

    // get the JWT from the file, if possible
    let jwt = tokio::fs::read_to_string(args.path.join(JWT_FILE))
        .await
        .ok();

    // create rpc channels
    let (client_response_in, client_transport, mut client_request_out) = RpcTransport::new();
    let (server_request_in, server_transport, mut server_response_out) = RpcTransport::new();

    // set up the client, facing the control plane
    let client =
        ControlServiceClient::new(tarpc::client::Config::default(), client_transport).spawn();

    // create the client state
    let state = Arc::new(GlobalState {
        client,
        external_addr,
        internal_addrs,
        cli: args,
        endpoint,
        jwt: Mutex::new(jwt),
        loki: Default::default(),
        env_info: Default::default(),
        agent_state: Default::default(),
        reconcilation_handle: Default::default(),
        child: Default::default(),
        resolved_addrs: Default::default(),
        metrics: Default::default(),
    });

    // start the metrics watcher
    metrics::init(Arc::clone(&state));

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

    // get the interrupt signals to break the stream connection
    let mut interrupt = Signals::new(&[SignalKind::terminate(), SignalKind::interrupt()]);

    'process: loop {
        'connection: {
            let mut req = ws_uri.to_owned().into_client_request().unwrap();

            // invalidate env info cache
            state.env_info.write().await.take();

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

            // attach agent key if one is set in env vars
            if let Ok(key) = std::env::var(ENV_AGENT_KEY) {
                req.headers_mut().insert(
                    HEADER_AGENT_KEY,
                    HeaderValue::from_bytes(key.as_bytes()).expect("attach agent key header"),
                );
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
                        if (ws_stream.send(tungstenite::Message::Binary(bin)).await).is_err() {
                            error!("The connection to the control plane was interrupted");
                            break 'event;
                        }
                    }

                    // handle outgoing requests
                    msg = client_request_out.recv() => {
                        let msg = msg.expect("internal RPC channel closed");
                        let bin = bincode::serialize(&MuxedMessageOutgoing::Control(msg)).expect("failed to serialize request");
                        if (ws_stream.send(tungstenite::Message::Binary(bin)).await).is_err() {
                            error!("The connection to the control plane was interrupted");
                            break 'event;
                        }
                    }

                    // handle incoming messages
                    msg = ws_stream.next() => match msg {
                        Some(Ok(tungstenite::Message::Binary(bin))) => {
                            let msg = match bincode::deserialize(&bin) {
                                Ok(msg) => msg,
                                Err(e) => {
                                    error!("failed to deserialize a message from the control plane: {e}");
                                    continue;
                                }
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

    state.node_graceful_shutdown().await;
    info!("snops agent has shut down gracefully :)");
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
