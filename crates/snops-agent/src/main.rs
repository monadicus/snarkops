mod api;
mod cli;
mod db;
mod metrics;
mod net;
mod reconcile;
mod rpc;
mod server;
mod state;
mod transfers;

use std::{
    mem::size_of,
    net::Ipv4Addr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use clap::Parser;
use cli::Cli;
use futures::SinkExt;
use futures_util::stream::{FuturesUnordered, StreamExt};
use http::HeaderValue;
use snops_common::{
    constant::{ENV_AGENT_KEY, HEADER_AGENT_KEY},
    db::Database,
    rpc::{agent::AgentService, control::ControlServiceClient, RpcTransport},
    util::OpaqueDebug,
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
use tracing::{error, info, level_filters::LevelFilter, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::rpc::{AgentRpcServer, MuxedMessageIncoming, MuxedMessageOutgoing};
use crate::state::GlobalState;

const PING_HEADER: &[u8] = b"snops-agent";
const PING_LENGTH: usize = size_of::<u32>() + size_of::<u128>();
const PING_INTERVAL_SEC: u64 = 10;

#[tokio::main]
async fn main() {
    let (stdout, _guard) = tracing_appender::non_blocking(std::io::stdout());
    let start_time = Instant::now();

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

    // For documentation purposes will exit after running the command.
    #[cfg(any(feature = "clipages", feature = "mangen"))]
    Cli::parse().run();
    let args = Cli::parse();

    let internal_addrs = match (args.internal, args.external) {
        // use specified internal address
        (Some(internal), _) => vec![internal],
        // use no internal address if the external address is loopback
        (None, Some(external)) if external.is_loopback() => vec![],
        // otherwise, get the local network interfaces available to this node
        (None, _) => net::get_internal_addrs().expect("failed to get network interfaces"),
    };
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

    // open the database
    let db = db::Database::open(args.path.join("store")).expect("failed to open database");

    // create rpc channels
    let (client_response_in, client_transport, mut client_request_out) = RpcTransport::new();
    let (server_request_in, server_transport, mut server_response_out) = RpcTransport::new();

    // set up the client, facing the control plane
    let client =
        ControlServiceClient::new(tarpc::client::Config::default(), client_transport).spawn();

    // start transfer monitor
    let (transfer_tx, transfers) = transfers::start_monitor(client.clone());

    let status_api_listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("failed to bind status server");
    let status_api_port = status_api_listener
        .local_addr()
        .expect("failed to get status server port")
        .port();

    // create the client state
    let state = Arc::new(GlobalState {
        client,
        db: OpaqueDebug(db),
        started: Instant::now(),
        connected: Mutex::new(Instant::now()),
        external_addr,
        internal_addrs,
        cli: args,
        endpoint,
        loki: Default::default(),
        env_info: Default::default(),
        agent_state: Default::default(),
        reconcilation_handle: Default::default(),
        child: Default::default(),
        resolved_addrs: Default::default(),
        metrics: Default::default(),
        status_api_port,
        transfer_tx,
        transfers,
    });

    // start the metrics watcher
    metrics::init(Arc::clone(&state));

    // start the status server
    let status_state = Arc::clone(&state);
    tokio::spawn(async move {
        info!("starting status API server on port {status_api_port}");
        if let Err(e) = server::start(status_api_listener, status_state).await {
            error!("status API server crashed: {e:?}");
            std::process::exit(1);
        }
    });

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
            if let Some(jwt) = state.db.jwt() {
                req.headers_mut().insert(
                    "Authorization",
                    HeaderValue::from_bytes(format!("Bearer {jwt}").as_bytes())
                        .expect("attach authorization header"),
                );
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

            *state.connected.lock().unwrap() = Instant::now();

            info!("Connection established with the control plane");

            let mut terminating = false;
            let mut interval = tokio::time::interval(Duration::from_secs(PING_INTERVAL_SEC));
            let mut num_pings: u32 = 0;

            'event: loop {
                select! {
                    // terminate if an interrupt was triggered
                    _ = interrupt.recv_any() => {
                        terminating = true;
                        break 'event;
                    }

                    _ = interval.tick() => {
                        // ping payload contains "snops-agent", number of pings, and uptime
                        let mut payload = Vec::from(PING_HEADER);
                        payload.extend_from_slice(&num_pings.to_le_bytes());
                        payload.extend_from_slice(&start_time.elapsed().as_micros().to_le_bytes());

                        let send = ws_stream.send(tungstenite::Message::Ping(payload));
                        if tokio::time::timeout(Duration::from_secs(10), send).await.is_err() {
                            error!("The connection to the control plane was interrupted while sending ping");
                            break 'event;
                        }
                    }

                    // handle outgoing responses
                    msg = server_response_out.recv() => {
                        let msg = msg.expect("internal RPC channel closed");
                        let bin = bincode::serialize(&MuxedMessageOutgoing::Agent(msg)).expect("failed to serialize response");
                        let send = ws_stream.send(tungstenite::Message::Binary(bin));
                        if tokio::time::timeout(Duration::from_secs(10), send).await.is_err() {
                            error!("The connection to the control plane was interrupted while sending agent message");
                            break 'event;
                        }
                    }

                    // handle outgoing requests
                    msg = client_request_out.recv() => {
                        let msg = msg.expect("internal RPC channel closed");
                        let bin = bincode::serialize(&MuxedMessageOutgoing::Control(msg)).expect("failed to serialize request");
                        let send = ws_stream.send(tungstenite::Message::Binary(bin));
                        if tokio::time::timeout(Duration::from_secs(10), send).await.is_err() {
                            error!("The connection to the control plane was interrupted while sending control message");
                            break 'event;
                        }
                    }

                    // handle incoming messages
                    msg = ws_stream.next() => match msg {
                        Some(Ok(tungstenite::Message::Close(frame))) => {
                            if let Some(frame) = frame {
                                info!("The control plane has closed the connection: {frame}");
                            } else {
                                info!("The control plane has closed the connection");
                            }
                            break 'event;
                        }

                        Some(Ok(tungstenite::Message::Pong(payload))) => {
                            let mut payload = payload.as_slice();
                            // check the header
                            if !payload.starts_with(PING_HEADER) {
                                warn!("Received a pong payload with an invalid header prefix");
                                continue;
                            }
                            payload = &payload[PING_HEADER.len()..];
                            if payload.len() != PING_LENGTH {
                                warn!("Received a pong payload with an invalid length {}, expected {PING_LENGTH}", payload.len());
                                continue;
                            }
                            let (left, right) = payload.split_at(size_of::<u32>());
                            let ping_index = u32::from_le_bytes(left.try_into().unwrap());
                            let _uptime_start = u128::from_le_bytes(right.try_into().unwrap());

                            if ping_index != num_pings {
                                warn!("Received a pong payload with an invalid index {ping_index}, expected {num_pings}");
                                continue;
                            }

                            num_pings += 1;

                            // when desired, we can add this as a metric
                            // let uptime_now = start_time.elapsed().as_micros();
                            // let uptime_diff = uptime_now - uptime_start;

                        }

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
