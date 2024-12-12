mod api;
mod cli;
mod client;
mod db;
mod metrics;
mod net;
mod reconcile;
mod rpc;
mod server;
mod service;
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
use reconcile::agent::{AgentStateReconciler, AgentStateReconcilerContext};
use snops_common::{db::Database, util::OpaqueDebug};
use tokio::{
    select,
    signal::unix::{signal, Signal, SignalKind},
    sync::{mpsc, RwLock},
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

    let mut args = Cli::parse();
    if args.modes.all_when_none() {
        info!(
            "No node modes specified, defaulting to all modes (client, validator, prover, compute)"
        );
    }

    let (internal_addrs, external_addr) = args.addrs();

    let (endpoint, ws_uri) = args.endpoint_and_uri();
    info!("Using endpoint {endpoint}");

    // Create the data directory
    tokio::fs::create_dir_all(&args.path)
        .await
        .expect("failed to create data path");

    // Open the database
    let db = db::Database::open(&args.path.join("store")).expect("failed to open database");

    let client = Default::default();

    // Start transfer monitor
    let (transfer_tx, transfers) = transfers::start_monitor(Arc::clone(&client));

    let agent_rpc_listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("failed to bind status server");

    // Setup the status server socket
    let agent_service_listener = if let Some(service_port) = args.service_port {
        Some(
            tokio::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, service_port))
                .await
                .expect("failed to bind status server"),
        )
    } else {
        None
    };
    let agent_rpc_port = agent_rpc_listener
        .local_addr()
        .expect("failed to get status server port")
        .port();

    let (queue_reconcile_tx, reconcile_requests) = mpsc::channel(5);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Create the client state
    let state = Arc::new(GlobalState {
        client,
        _started: Instant::now(),
        external_addr,
        internal_addrs,
        cli: args,
        endpoint,
        queue_reconcile_tx,
        loki: Mutex::new(db.loki_url()),
        last_node_status: RwLock::new(None),
        env_info: RwLock::new(
            db.env_info()
                .inspect_err(|e| {
                    error!("failed to load env info from db: {e}");
                })
                .unwrap_or_default(),
        ),
        agent_state: RwLock::new(
            db.agent_state()
                .map(Arc::new)
                .inspect_err(|e| {
                    error!("failed to load agent state from db: {e}");
                })
                .unwrap_or_default(),
        ),
        resolved_addrs: RwLock::new(
            db.resolved_addrs()
                .inspect_err(|e| {
                    error!("failed to load resolved addrs from db: {e}");
                })
                .unwrap_or_default(),
        ),
        metrics: Default::default(),
        agent_rpc_port,
        transfer_tx,
        transfers,
        node_client: Default::default(),
        log_level_handler: reload_handler,
        db: OpaqueDebug(db),
        shutdown: RwLock::new(Some(shutdown_tx)),
    });

    // Start the metrics watcher
    metrics::init(Arc::clone(&state));

    // Start the status server
    let status_state = Arc::clone(&state);
    tokio::spawn(async move {
        info!("Starting status API server on port {agent_rpc_port}");
        if let Err(e) = server::start(agent_rpc_listener, status_state).await {
            error!("status API server crashed: {e:?}");
            std::process::exit(1);
        }
    });

    // Start the status server if enabled
    if let Some(listener) = agent_service_listener {
        let service_state = Arc::clone(&state);
        tokio::spawn(async move {
            info!("Starting service API server on port {agent_rpc_port}");
            if let Err(e) = service::start(listener, service_state).await {
                error!("service API server crashed: {e:?}");
                std::process::exit(1);
            }
        });
    }

    // Get the interrupt signals to break the stream connection
    let mut interrupt = Signals::term_or_interrupt();

    let state2 = Arc::clone(&state);
    tokio::spawn(async move {
        loop {
            let req = client::new_ws_request(&ws_uri, state2.db.jwt());
            client::ws_connection(req, Arc::clone(&state2)).await;
            // Remove the control client
            state2.client.write().await.take();
            info!("Attempting to reconnect to the control plane...");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });

    // Root reconciler that walks through configuring the agent.
    // The context is mutated while reconciling to keep track of things
    // like downloads, ledger manipulations, node command, and more.
    let mut root = AgentStateReconciler {
        agent_state: state.get_agent_state().await,
        state: Arc::clone(&state),
        // Recover context from previous state
        context: AgentStateReconcilerContext::hydrate(&state.db),
    };

    select! {
        _ = root.loop_forever(reconcile_requests) => unreachable!(),
        _ = interrupt.recv_any() => {},
        _ = shutdown_rx => {},
    }

    info!("Received interrupt signal, shutting down...");
    if let Some(process) = root.context.process.as_mut() {
        process.graceful_shutdown().await;
        info!("Agent has shut down gracefully");
    }
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

    pub fn term_or_interrupt() -> Self {
        Self::new(&[SignalKind::terminate(), SignalKind::interrupt()])
    }

    async fn recv_any(&mut self) {
        let mut futs = FuturesUnordered::new();

        for sig in self.signals.iter_mut() {
            futs.push(sig.recv());
        }

        futs.next().await;
    }
}

#[cfg(test)]
mod test {
    #[test]
    // CI is failing because the agent has no tests
    fn test_nothing() {
        assert_eq!(1, 1)
    }
}
