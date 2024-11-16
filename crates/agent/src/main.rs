mod api;
mod cli;
mod client;
mod db;
mod metrics;
mod net;
mod reconcile;
mod rpc;
mod server;
mod state;
mod transfers;

use std::{
    net::Ipv4Addr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use clap::Parser;
use cli::Cli;
use futures_util::stream::{FuturesUnordered, StreamExt};
use log::init_logging;
use snops_common::{db::Database, util::OpaqueDebug};
use tokio::{
    select,
    signal::unix::{signal, Signal, SignalKind},
    sync::RwLock,
};
use tracing::{error, info};

use crate::state::GlobalState;
mod log;

#[tokio::main]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // For documentation purposes will exit after running the command.
    #[cfg(any(feature = "clipages", feature = "mangen"))]
    Cli::parse().run();

    let (_guard, reload_handler) = init_logging();

    let args = Cli::parse();

    let (internal_addrs, external_addr) = args.addrs();

    let (endpoint, ws_uri) = args.endpoint_and_uri();
    info!("Using endpoint {endpoint}");

    // create the data directory
    tokio::fs::create_dir_all(&args.path)
        .await
        .expect("failed to create data path");

    // open the database
    let db = db::Database::open(&args.path.join("store")).expect("failed to open database");

    let client = Default::default();

    // start transfer monitor
    let (transfer_tx, transfers) = transfers::start_monitor(Arc::clone(&client));

    let agent_rpc_listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("failed to bind status server");
    let agent_rpc_port = agent_rpc_listener
        .local_addr()
        .expect("failed to get status server port")
        .port();

    // create the client state
    let state = Arc::new(GlobalState {
        client,
        _started: Instant::now(),
        external_addr,
        internal_addrs,
        cli: args,
        endpoint,
        loki: Mutex::new(db.loki_url()),
        env_info: RwLock::new(
            db.env_info()
                .inspect_err(|e| {
                    error!("failed to load env info from db: {e}");
                })
                .unwrap_or_default(),
        ),
        agent_state: RwLock::new(
            db.agent_state()
                .inspect_err(|e| {
                    error!("failed to load agent state from db: {e}");
                })
                .unwrap_or_default(),
        ),
        reconcilation_handle: Default::default(),
        child: Default::default(),
        resolved_addrs: Default::default(),
        metrics: Default::default(),
        agent_rpc_port,
        transfer_tx,
        transfers,
        node_client: Default::default(),
        log_level_handler: reload_handler,
        db: OpaqueDebug(db),
    });

    // start the metrics watcher
    metrics::init(Arc::clone(&state));

    // start the status server
    let status_state = Arc::clone(&state);
    tokio::spawn(async move {
        info!("starting status API server on port {agent_rpc_port}");
        if let Err(e) = server::start(agent_rpc_listener, status_state).await {
            error!("status API server crashed: {e:?}");
            std::process::exit(1);
        }
    });

    // get the interrupt signals to break the stream connection
    let mut interrupt = Signals::new(&[SignalKind::terminate(), SignalKind::interrupt()]);

    let state2 = Arc::clone(&state);
    let connection_loop = Box::pin(async move {
        loop {
            let req = client::new_ws_request(&ws_uri, state2.db.jwt());
            client::ws_connection(req, Arc::clone(&state2)).await;
            info!("Attempting to reconnect...");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });

    select! {
        _ = interrupt.recv_any() => {
            info!("Received interrupt signal, shutting down...");
        },

        _ = connection_loop => unreachable!()
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
